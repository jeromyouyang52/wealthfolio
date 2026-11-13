//! Contribution room for TFSAs, RRSPs and FHSAs.
//!
//! The app records what went in and out of each registered account; the CRA
//! knows what happened before that and outside the app. So every calculation
//! starts from a figure the holder copies from the CRA (or, for a TFSA, the
//! year they became eligible, as an estimate) and then follows the recorded
//! money in Canadian dollars.
//!
//! The formulas follow the definitions in subsection 207.01(1) of the Income
//! Tax Act:
//!
//! - TFSA: the excess at a moment of year Y is the year's contributions so far,
//!   less the room on January 1, less the part of each withdrawal this year
//!   that brought an excess down. Room on January 1 is the previous year's
//!   room, less its contributions, plus its withdrawals, plus the year's limit.
//!   A direct transfer between the holder's own TFSAs is neither.
//! - FHSA: the room for a year is the lesser of $8,000 plus the carryforward
//!   less last year's closing excess, and $40,000 less everything contributed
//!   or transferred in before the year. The carryforward is at most $8,000.
//! - RRSP: the room is the "available contribution room" printed on the notice
//!   of assessment for a year. Contributions made from the 61st day of that
//!   year to the 60th day of the next use it; the ones in the first 60 days
//!   were reported on the previous return and are already in the figure.
//!
//! Over-contributions are taxed at 1% a month on the highest excess in the
//! month (above the $2,000 cushion for an RRSP); the estimates here follow
//! that rule and stop at the current month.

use std::collections::BTreeMap;

use chrono::{Datelike, Days, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::rules::Rules;

/// The plans this module tracks room for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Plan {
    Tfsa,
    Rrsp,
    Fhsa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowKind {
    /// Money or property put into the plan.
    Contribution,
    /// Money or property taken out to the holder.
    Withdrawal,
    /// A direct transfer from the holder's RRSP into their FHSA. It uses FHSA
    /// room like a contribution.
    RrspTransfer,
}

/// One recorded movement into or out of a plan, in Canadian dollars.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanFlow {
    pub activity_id: String,
    pub date: NaiveDate,
    pub kind: FlowKind,
    pub amount_cad: Decimal,
}

/// Where a holder's TFSA room starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "basis", rename_all = "camelCase", deny_unknown_fields)]
pub enum TfsaStart {
    /// "Contribution room on January 1" as the CRA shows it for `year`.
    #[serde(rename_all = "camelCase")]
    CraRoom { year: i32, room: Decimal },
    /// The first year the holder was 18 or older and resident in Canada.
    /// Assumes every contribution and withdrawal since is recorded here.
    #[serde(rename_all = "camelCase")]
    EligibleSince { year: i32 },
}

/// The RRSP figure from a notice of assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RrspStart {
    /// The year the room is for (the notice for the previous tax year).
    pub year: i32,
    /// "Available contribution room": the deduction limit less unused
    /// contributions already reported.
    pub available_room: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FhsaStart {
    /// The year the holder opened their first FHSA.
    pub opened_year: i32,
}

/// What a holder told the app about their room. Each plan is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HolderProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tfsa: Option<TfsaStart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rrsp: Option<RrspStart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fhsa: Option<FhsaStart>,
}

impl HolderProfile {
    /// Reads and checks a stored or submitted profile.
    pub fn from_json(json: &str) -> Result<Self> {
        let profile: Self =
            serde_json::from_str(json).map_err(|e| Error::InvalidProfile(e.to_string()))?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| Error::InvalidProfile(e.to_string()))
    }

    pub fn validate(&self) -> Result<()> {
        let invalid = |what: &str| Err(Error::InvalidProfile(what.to_string()));
        let plausible_year = |year: i32| (1900..=2200).contains(&year);
        match &self.tfsa {
            Some(TfsaStart::CraRoom { year, .. }) if *year < 2009 || !plausible_year(*year) => {
                return invalid("the TFSA year must be 2009 or later");
            }
            Some(TfsaStart::EligibleSince { year }) if !plausible_year(*year) => {
                return invalid("the TFSA eligibility year is not a year");
            }
            _ => {}
        }
        if let Some(rrsp) = &self.rrsp {
            if !plausible_year(rrsp.year) {
                return invalid("the RRSP year is not a year");
            }
        }
        if let Some(fhsa) = &self.fhsa {
            if fhsa.opened_year < 2023 || !plausible_year(fhsa.opened_year) {
                return invalid("FHSAs exist from 2023");
            }
        }
        Ok(())
    }
}

/// The highest excess in one month and the tax it attracts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcessMonth {
    /// The first day of the month.
    pub month: NaiveDate,
    pub highest_excess_cad: Decimal,
    pub tax_cad: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TfsaYear {
    pub year: i32,
    /// Room on January 1. Negative when an excess was carried in.
    pub room_jan1_cad: Decimal,
    pub contributions_cad: Decimal,
    pub withdrawals_cad: Decimal,
    pub excess_months: Vec<ExcessMonth>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TfsaRoom {
    /// `craRoom` or `eligibleSince`: the second is an estimate.
    pub basis: &'static str,
    pub year: i32,
    pub room_jan1_cad: Decimal,
    pub contributions_cad: Decimal,
    pub withdrawals_cad: Decimal,
    /// What can still go in this year. Negative means an excess now.
    pub available_cad: Decimal,
    pub excess_cad: Decimal,
    /// Room on January 1 next year if nothing else happens; `None` when the
    /// rule table has no limit for next year yet.
    pub next_year_room_cad: Option<Decimal>,
    pub estimated_tax_cad: Decimal,
    /// Recorded flows before the starting year, which the CRA figure already
    /// reflects and so are left out.
    pub flows_before_start: usize,
    pub years: Vec<TfsaYear>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RrspRoom {
    pub year: i32,
    pub available_room_cad: Decimal,
    /// The contribution window for the figure: day 61 of `year` to day 60 of
    /// the next year.
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub contributions_cad: Decimal,
    /// Negative means over the room.
    pub remaining_cad: Decimal,
    /// The $2,000 an RRSP can be over without tax.
    pub cushion_cad: Decimal,
    pub excess_months: Vec<ExcessMonth>,
    pub estimated_tax_cad: Decimal,
    /// Contributions in the first 60 days of `year`: already in the figure.
    pub before_window_cad: Decimal,
    /// Contributions after the window: they need the next notice's figure.
    pub after_window_cad: Decimal,
    /// The window has closed; a newer notice has the current figure.
    pub outdated: bool,
    /// The dollar limit for next year, the most new room can be.
    pub next_year_dollar_limit_cad: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FhsaYear {
    pub year: i32,
    pub room_cad: Decimal,
    pub carryforward_cad: Decimal,
    pub contributions_cad: Decimal,
    pub rrsp_transfers_cad: Decimal,
    pub excess_months: Vec<ExcessMonth>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FhsaRoom {
    pub opened_year: i32,
    pub year: i32,
    pub room_cad: Decimal,
    pub carryforward_cad: Decimal,
    pub contributions_cad: Decimal,
    pub rrsp_transfers_cad: Decimal,
    /// Negative means an excess now.
    pub remaining_cad: Decimal,
    pub excess_cad: Decimal,
    pub lifetime_used_cad: Decimal,
    pub lifetime_remaining_cad: Decimal,
    /// Next year's room if nothing else happens this year.
    pub next_year_room_cad: Decimal,
    pub estimated_tax_cad: Decimal,
    /// The account can stay an FHSA for at most 15 years after it opened.
    pub participation_period_may_have_ended: bool,
    /// Withdrawals are recorded but not modelled (see the report's issues).
    pub withdrawals_cad: Decimal,
    pub years: Vec<FhsaYear>,
}

fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

fn jan1(year: i32) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(year, 1, 1).ok_or(Error::InvalidProfile("year out of range".into()))
}

/// The 60th day of `year` (March 1, or February 29 in a leap year).
pub fn sixtieth_day(year: i32) -> Result<NaiveDate> {
    jan1(year)?
        .checked_add_days(Days::new(59))
        .ok_or(Error::InvalidProfile("year out of range".into()))
}

/// Tracks the highest excess per month while events are applied in order.
struct MonthlyExcess {
    rate: Decimal,
    months: BTreeMap<NaiveDate, Decimal>,
}

impl MonthlyExcess {
    fn new(rate: Decimal) -> Self {
        Self {
            rate,
            months: BTreeMap::new(),
        }
    }

    /// Records that `excess` stood at some moment of `date`'s month.
    fn observe(&mut self, date: NaiveDate, excess: Decimal) {
        if excess <= Decimal::ZERO {
            return;
        }
        let entry = self
            .months
            .entry(first_of_month(date))
            .or_insert(Decimal::ZERO);
        *entry = (*entry).max(excess);
    }

    /// Records that `excess` stood throughout every month from `from` to
    /// `to`, both inclusive.
    fn hold(&mut self, from: NaiveDate, to: NaiveDate, excess: Decimal) {
        let mut month = first_of_month(from);
        while month <= to {
            self.observe(month, excess);
            month = match month.checked_add_months(chrono::Months::new(1)) {
                Some(next) => next,
                None => break,
            };
        }
    }

    fn finish(self) -> Vec<ExcessMonth> {
        self.months
            .into_iter()
            .map(|(month, highest)| ExcessMonth {
                month,
                highest_excess_cad: highest,
                tax_cad: (highest * self.rate).round_dp(2),
            })
            .collect()
    }
}

fn sorted_flows(flows: &[PlanFlow]) -> Vec<&PlanFlow> {
    let mut sorted: Vec<&PlanFlow> = flows.iter().collect();
    // On one day, money in is applied before money out: the month's highest
    // excess is the one that counts, and this order never understates it.
    sorted.sort_by_key(|flow| (flow.date, flow.kind == FlowKind::Withdrawal));
    sorted
}

fn tax_total(years: impl IntoIterator<Item = Decimal>) -> Decimal {
    years.into_iter().sum()
}

/// TFSA room from `start` through `today`'s year, following `flows` (the
/// holder's TFSA contributions and withdrawals, in any order).
pub fn tfsa_room(
    start: &TfsaStart,
    flows: &[PlanFlow],
    rules: &Rules,
    today: NaiveDate,
) -> Result<TfsaRoom> {
    let limit = |year: i32| {
        rules.tfsa_annual_limit(year).ok_or(Error::MissingRule {
            plan: Plan::Tfsa,
            year,
        })
    };
    let first_table_year = rules
        .tfsa
        .annual_limits
        .keys()
        .next()
        .copied()
        .unwrap_or(2009);
    let current_year = today.year();
    let first_year = match start {
        TfsaStart::CraRoom { year, .. } => *year,
        TfsaStart::EligibleSince { year } => (*year).max(first_table_year),
    };
    if first_year > current_year {
        return Err(Error::InvalidProfile(
            "the TFSA start is in the future".into(),
        ));
    }
    let (basis, mut room_jan1) = match start {
        TfsaStart::CraRoom { room, .. } => ("craRoom", *room),
        TfsaStart::EligibleSince { .. } => ("eligibleSince", limit(first_year)?),
    };

    let sorted = sorted_flows(flows);
    let flows_before_start = sorted.iter().filter(|f| f.date.year() < first_year).count();
    let rate = rules.tfsa.over_contribution_monthly_rate;
    let mut years = Vec::new();

    for year in first_year..=current_year {
        if let Some(previous) = years.last() {
            let previous: &TfsaYear = previous;
            room_jan1 = previous.room_jan1_cad - previous.contributions_cad
                + previous.withdrawals_cad
                + limit(year)?;
        }
        let year_end = if year == current_year {
            today
        } else {
            NaiveDate::from_ymd_opt(year, 12, 31).unwrap_or(today)
        };
        let mut months = MonthlyExcess::new(rate);
        let mut contributed = Decimal::ZERO;
        let mut withdrawn = Decimal::ZERO;
        // Withdrawals' "qualifying portions": the part that cut an excess.
        let mut reduced = Decimal::ZERO;
        let excess = |contributed: Decimal, reduced: Decimal| {
            (contributed - room_jan1 - reduced).max(Decimal::ZERO)
        };
        let mut cursor = jan1(year)?;
        for flow in sorted
            .iter()
            .filter(|f| f.date.year() == year && f.date <= year_end)
        {
            // The excess before this flow stood since the last one.
            months.hold(cursor, flow.date, excess(contributed, reduced));
            match flow.kind {
                FlowKind::Contribution | FlowKind::RrspTransfer => contributed += flow.amount_cad,
                FlowKind::Withdrawal => {
                    let before = excess(contributed, reduced);
                    reduced += flow.amount_cad.min(before);
                    withdrawn += flow.amount_cad;
                }
            }
            months.observe(flow.date, excess(contributed, reduced));
            cursor = flow.date;
        }
        months.hold(cursor, year_end, excess(contributed, reduced));
        years.push(TfsaYear {
            year,
            room_jan1_cad: room_jan1,
            contributions_cad: contributed,
            withdrawals_cad: withdrawn,
            excess_months: months.finish(),
        });
        if year == current_year {
            let available = room_jan1 + reduced - contributed;
            let next_year_room = rules
                .tfsa_annual_limit(year + 1)
                .map(|next| room_jan1 - contributed + withdrawn + next);
            return Ok(TfsaRoom {
                basis,
                year,
                room_jan1_cad: room_jan1,
                contributions_cad: contributed,
                withdrawals_cad: withdrawn,
                available_cad: available,
                excess_cad: (-available).max(Decimal::ZERO),
                next_year_room_cad: next_year_room,
                estimated_tax_cad: tax_total(
                    years
                        .iter()
                        .flat_map(|y| y.excess_months.iter().map(|m| m.tax_cad)),
                ),
                flows_before_start,
                years,
            });
        }
    }
    Err(Error::InvalidProfile(
        "the TFSA start is in the future".into(),
    ))
}

/// RRSP room left from a notice of assessment's figure.
pub fn rrsp_room(
    start: &RrspStart,
    flows: &[PlanFlow],
    rules: &Rules,
    today: NaiveDate,
) -> Result<RrspRoom> {
    let window_start = sixtieth_day(start.year)?
        .checked_add_days(Days::new(1))
        .ok_or(Error::InvalidProfile("year out of range".into()))?;
    let window_end = sixtieth_day(start.year + 1)?;
    let cushion = rules.rrsp.over_contribution_buffer;
    let year_end = NaiveDate::from_ymd_opt(start.year, 12, 31).unwrap_or(window_end);
    // From January 1 the next year's room (unknown here) adds to this one, so
    // the tax estimate stops at the end of the year.
    let tax_until = today.min(year_end);

    let mut months = MonthlyExcess::new(rules.rrsp.over_contribution_monthly_rate);
    let over =
        |contributed: Decimal| (contributed - start.available_room - cushion).max(Decimal::ZERO);
    let mut contributed = Decimal::ZERO;
    // Contributed by the last day the tax estimate covers.
    let mut contributed_taxed = Decimal::ZERO;
    let mut before_window = Decimal::ZERO;
    let mut after_window = Decimal::ZERO;
    let mut cursor = window_start;
    for flow in sorted_flows(flows) {
        if flow.kind != FlowKind::Contribution {
            continue;
        }
        if flow.date.year() == start.year && flow.date < window_start {
            before_window += flow.amount_cad;
        } else if flow.date > window_end {
            after_window += flow.amount_cad;
        } else if flow.date >= window_start {
            contributed += flow.amount_cad;
            if flow.date <= tax_until {
                months.hold(cursor, flow.date, over(contributed_taxed));
                contributed_taxed = contributed;
                months.observe(flow.date, over(contributed_taxed));
                cursor = flow.date;
            }
        }
    }
    if cursor <= tax_until {
        months.hold(cursor, tax_until, over(contributed_taxed));
    }
    let excess_months = months.finish();
    Ok(RrspRoom {
        year: start.year,
        available_room_cad: start.available_room,
        window_start,
        window_end,
        contributions_cad: contributed,
        remaining_cad: start.available_room - contributed,
        cushion_cad: cushion,
        estimated_tax_cad: tax_total(excess_months.iter().map(|m| m.tax_cad)),
        excess_months,
        before_window_cad: before_window,
        after_window_cad: after_window,
        outdated: today > window_end,
        next_year_dollar_limit_cad: rules.rrsp.dollar_limits.get(&(start.year + 1)).copied(),
    })
}

/// FHSA participation room from the year the first FHSA opened.
pub fn fhsa_room(
    start: &FhsaStart,
    flows: &[PlanFlow],
    rules: &Rules,
    today: NaiveDate,
) -> Result<FhsaRoom> {
    let current_year = today.year();
    if start.opened_year > current_year {
        return Err(Error::InvalidProfile(
            "the FHSA opening year is in the future".into(),
        ));
    }
    if start.opened_year < rules.fhsa.first_year {
        return Err(Error::InvalidProfile(
            "FHSAs did not exist that year".into(),
        ));
    }
    let annual = rules.fhsa.annual_limit;
    let lifetime = rules.fhsa.lifetime_limit;
    let max_carry = rules.fhsa.max_carry_forward;
    let rate = rules.fhsa.over_contribution_monthly_rate;
    let sorted = sorted_flows(flows);

    let mut years: Vec<FhsaYear> = Vec::new();
    // Everything contributed or transferred in before the year being computed.
    let mut prior_total = Decimal::ZERO;
    let mut carryforward = Decimal::ZERO;
    let mut excess_end_previous = Decimal::ZERO;
    let mut excess_end_second_previous = Decimal::ZERO;
    let mut withdrawals = Decimal::ZERO;
    let mut room = Decimal::ZERO;
    let mut room_unfloored_last = Decimal::ZERO;
    let mut used_this_year = Decimal::ZERO;

    for year in start.opened_year..=current_year {
        if let Some(previous) = years.last() {
            let previous: &FhsaYear = previous;
            let previous_used = previous.contributions_cad + previous.rrsp_transfers_cad;
            carryforward =
                (annual + previous.carryforward_cad - excess_end_second_previous - previous_used)
                    .max(Decimal::ZERO)
                    .min(max_carry);
            prior_total += previous_used;
        }
        // Negative when an excess is carried past the new room: it stays an
        // excess until room absorbs it.
        let room_unfloored =
            (annual + carryforward - excess_end_previous).min(lifetime - prior_total);
        room = room_unfloored.max(Decimal::ZERO);
        room_unfloored_last = room_unfloored;

        let year_end = if year == current_year {
            today
        } else {
            NaiveDate::from_ymd_opt(year, 12, 31).unwrap_or(today)
        };
        let mut months = MonthlyExcess::new(rate);
        let mut contributions = Decimal::ZERO;
        let mut transfers = Decimal::ZERO;
        let excess = |used: Decimal| (used - room_unfloored).max(Decimal::ZERO);
        let mut cursor = jan1(year)?;
        for flow in sorted
            .iter()
            .filter(|f| f.date.year() == year && f.date <= year_end)
        {
            months.hold(cursor, flow.date, excess(contributions + transfers));
            match flow.kind {
                FlowKind::Contribution => contributions += flow.amount_cad,
                FlowKind::RrspTransfer => transfers += flow.amount_cad,
                FlowKind::Withdrawal => withdrawals += flow.amount_cad,
            }
            months.observe(flow.date, excess(contributions + transfers));
            cursor = flow.date;
        }
        months.hold(cursor, year_end, excess(contributions + transfers));
        used_this_year = contributions + transfers;
        excess_end_second_previous = excess_end_previous;
        excess_end_previous = excess(used_this_year);
        years.push(FhsaYear {
            year,
            room_cad: room,
            carryforward_cad: carryforward,
            contributions_cad: contributions,
            rrsp_transfers_cad: transfers,
            excess_months: months.finish(),
        });
    }

    let last = years
        .last()
        .expect("the loop runs at least once: the opening year is not after this year");
    let lifetime_used = prior_total + used_this_year;
    let next_carry = (annual + last.carryforward_cad - excess_end_second_previous - used_this_year)
        .max(Decimal::ZERO)
        .min(max_carry);
    let next_year_room = (annual + next_carry - excess_end_previous)
        .min(lifetime - lifetime_used)
        .max(Decimal::ZERO);
    let excess_now = excess_end_previous;
    Ok(FhsaRoom {
        opened_year: start.opened_year,
        year: current_year,
        room_cad: room,
        carryforward_cad: last.carryforward_cad,
        contributions_cad: last.contributions_cad,
        rrsp_transfers_cad: last.rrsp_transfers_cad,
        remaining_cad: room_unfloored_last - used_this_year,
        excess_cad: excess_now,
        lifetime_used_cad: lifetime_used,
        lifetime_remaining_cad: (lifetime - lifetime_used).max(Decimal::ZERO),
        next_year_room_cad: next_year_room,
        estimated_tax_cad: tax_total(
            years
                .iter()
                .flat_map(|y| y.excess_months.iter().map(|m| m.tax_cad)),
        ),
        participation_period_may_have_ended: current_year > start.opened_year + 15,
        withdrawals_cad: withdrawals,
        years,
    })
}

/// Every holder's profile, keyed by holder, as the app stores them.
pub type HolderProfiles = BTreeMap<String, HolderProfile>;

/// Reads the stored profiles; each is checked as when it was saved.
pub fn profiles_from_json(json: &str) -> Result<HolderProfiles> {
    let profiles: HolderProfiles =
        serde_json::from_str(json).map_err(|e| Error::InvalidProfile(e.to_string()))?;
    for (holder, profile) in &profiles {
        check_holder(holder)?;
        profile.validate()?;
    }
    Ok(profiles)
}

pub fn profiles_to_json(profiles: &HolderProfiles) -> Result<String> {
    serde_json::to_string(profiles).map_err(|e| Error::InvalidProfile(e.to_string()))
}

/// A holder is the short name an account's Canadian settings carry.
pub fn check_holder(holder: &str) -> Result<()> {
    let trimmed = holder.trim();
    if trimmed.is_empty() || trimmed != holder || holder.chars().count() > 64 {
        return Err(Error::InvalidProfile(
            "the holder name is empty or too long".into(),
        ));
    }
    Ok(())
}
