//! The dated history of one security for one holder, and the superficial
//! loss rule.
//!
//! A loss is superficial when the holder, or a person affiliated with them
//! (their own registered plans count), acquires the identical property in the
//! period from 30 days before to 30 days after the sale and still owns it at
//! the end of that period. The CRA's formula denies
//!
//! ```text
//! (least of S, P, B) / S x L
//! ```
//!
//! where S is the units sold, P the units acquired in the period, B the units
//! owned at the end of it and L the loss. The denied part is added to the cost
//! of the shares kept. When they are kept in a registered plan they have no
//! adjusted cost base to add it to, and that part of the loss is gone.
//!
//! The rule looks ahead, so a loss sold less than 30 days before `as_of` is
//! provisional: a later purchase can still deny it.
//!
//! Choices the formula leaves open, each flagged on the record it affects:
//! - Shares kept partly inside and partly outside registered plans: the denied
//!   loss is split by where the units sit at the end of the period
//!   (`needs_review`).
//! - Each loss is tested on its own. Two losses in overlapping periods can
//!   both count the same purchase.

use chrono::{Datelike, Days, NaiveDate};
use rust_decimal::Decimal;

use crate::acb::AcbPool;
use crate::error::{Error, Result};
use crate::rules::{Rules, SuperficialLossRules};

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub date: NaiveDate,
    pub kind: EventKind,
}

/// What happened, in Canadian dollars. Events of the holder's non-registered
/// accounts move the pool; the two registered kinds only count units, for the
/// superficial loss test.
#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    /// A purchase, a reinvested dividend that issued units, a transfer in.
    Acquire {
        units: Decimal,
        cost_cad: Decimal,
    },
    Dispose {
        units: Decimal,
        net_proceeds_cad: Decimal,
    },
    /// Applies to the registered units too: it is the same security.
    Split {
        ratio: Decimal,
    },
    ReturnOfCapital {
        amount_cad: Decimal,
    },
    /// Income reinvested without new units (a "phantom" distribution).
    NotionalDistribution {
        amount_cad: Decimal,
    },
    RegisteredAcquire {
        units: Decimal,
    },
    RegisteredDispose {
        units: Decimal,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DispositionRecord {
    pub date: NaiveDate,
    pub units: Decimal,
    pub net_proceeds_cad: Decimal,
    pub acb_relieved_cad: Decimal,
    /// Before the superficial loss rule; negative for a loss.
    pub gain_cad: Decimal,
    /// The part of the loss the rule denies (zero or positive).
    pub denied_loss_cad: Decimal,
    /// The part of the denied loss carried into the cost of the shares kept.
    pub denied_added_to_acb_cad: Decimal,
    /// The part of the denied loss gone for good: shares kept in a registered plan.
    pub denied_lost_cad: Decimal,
    /// The 30 days after the sale had not all passed at `as_of`.
    pub provisional: bool,
    /// The shares kept sit both inside and outside registered plans.
    pub needs_review: bool,
}

impl DispositionRecord {
    /// The gain or loss to report: the loss less its denied part.
    pub fn reportable_gain_cad(&self) -> Decimal {
        self.gain_cad + self.denied_loss_cad
    }
}

/// A capital gain with no sale: a return of capital beyond the cost.
#[derive(Debug, Clone, PartialEq)]
pub struct DeemedGain {
    pub date: NaiveDate,
    pub gain_cad: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ledger {
    pub dispositions: Vec<DispositionRecord>,
    pub deemed_gains: Vec<DeemedGain>,
    /// The position after the last event.
    pub pool: AcbPool,
}

/// Units after an event, and the product of every split ratio up to it.
#[derive(Debug, Clone, Copy)]
struct Units {
    non_registered: Decimal,
    registered: Decimal,
    split_factor: Decimal,
}

impl Ledger {
    /// Replays `events` (sorted here by date; events of one day keep their
    /// order) and applies the superficial loss rule as known at `as_of`.
    pub fn compute(
        mut events: Vec<Event>,
        rule: &SuperficialLossRules,
        as_of: NaiveDate,
    ) -> Result<Self> {
        events.sort_by_key(|event| event.date);
        let units = unit_timeline(&events)?;

        let mut pool = AcbPool::new();
        let mut dispositions = Vec::new();
        let mut deemed_gains = Vec::new();
        // A denied loss waiting for the purchase that replaces shares sold out.
        let mut carried_denied = Decimal::ZERO;

        for (index, event) in events.iter().enumerate() {
            match &event.kind {
                EventKind::Acquire { units, cost_cad } => {
                    pool.acquire(*units, *cost_cad)?;
                    if carried_denied > Decimal::ZERO {
                        pool.increase_cost(carried_denied)?;
                        carried_denied = Decimal::ZERO;
                    }
                }
                EventKind::Split { ratio } => pool.split(*ratio)?,
                EventKind::ReturnOfCapital { amount_cad } => {
                    let excess = pool.return_of_capital(*amount_cad)?;
                    if excess > Decimal::ZERO {
                        deemed_gains.push(DeemedGain {
                            date: event.date,
                            gain_cad: excess,
                        });
                    }
                }
                EventKind::NotionalDistribution { amount_cad } => {
                    pool.increase_cost(*amount_cad)?;
                }
                EventKind::RegisteredAcquire { .. } | EventKind::RegisteredDispose { .. } => {}
                EventKind::Dispose {
                    units: sold,
                    net_proceeds_cad,
                } => {
                    let sale = pool.dispose(*sold, *net_proceeds_cad)?;
                    let window_end = event.date + Days::new(u64::from(rule.days_after));
                    let mut record = DispositionRecord {
                        date: event.date,
                        units: sale.units,
                        net_proceeds_cad: sale.net_proceeds_cad,
                        acb_relieved_cad: sale.acb_relieved_cad,
                        gain_cad: sale.gain_cad,
                        denied_loss_cad: Decimal::ZERO,
                        denied_added_to_acb_cad: Decimal::ZERO,
                        denied_lost_cad: Decimal::ZERO,
                        provisional: sale.gain_cad < Decimal::ZERO && window_end > as_of,
                        needs_review: false,
                    };
                    if sale.gain_cad < Decimal::ZERO {
                        let kept = kept_after_loss(&events, &units, index, rule);
                        let counted = sale.units.min(kept.acquired).min(kept.held());
                        if counted > Decimal::ZERO {
                            let denied = -sale.gain_cad * counted / sale.units;
                            let to_acb = denied * kept.held_non_registered / kept.held();
                            record.denied_loss_cad = denied;
                            record.denied_added_to_acb_cad = to_acb;
                            record.denied_lost_cad = denied - to_acb;
                            record.needs_review = kept.held_non_registered > Decimal::ZERO
                                && kept.held_registered > Decimal::ZERO;
                            if to_acb > Decimal::ZERO {
                                if pool.units() > Decimal::ZERO {
                                    pool.increase_cost(to_acb)?;
                                } else {
                                    // Sold out: non-registered shares are held at
                                    // the end of the period, so a purchase follows.
                                    carried_denied += to_acb;
                                }
                            }
                        }
                    }
                    dispositions.push(record);
                }
            }
        }

        Ok(Self {
            dispositions,
            deemed_gains,
            pool,
        })
    }
}

/// Counts units through the history. Units do not depend on cost, so the
/// superficial loss test can read the holdings of a later day.
fn unit_timeline(events: &[Event]) -> Result<Vec<Units>> {
    let mut now = Units {
        non_registered: Decimal::ZERO,
        registered: Decimal::ZERO,
        split_factor: Decimal::ONE,
    };
    let mut timeline = Vec::with_capacity(events.len());
    for event in events {
        match &event.kind {
            EventKind::Acquire { units, .. } => {
                require_positive(*units, "Units acquired")?;
                now.non_registered += *units;
            }
            EventKind::Dispose { units, .. } => {
                now.non_registered = remove(now.non_registered, *units)?;
            }
            EventKind::RegisteredAcquire { units } => {
                require_positive(*units, "Units acquired")?;
                now.registered += *units;
            }
            EventKind::RegisteredDispose { units } => {
                now.registered = remove(now.registered, *units)?;
            }
            EventKind::Split { ratio } => {
                require_positive(*ratio, "A split ratio")?;
                now.non_registered *= *ratio;
                now.registered *= *ratio;
                now.split_factor *= *ratio;
            }
            EventKind::ReturnOfCapital { .. } | EventKind::NotionalDistribution { .. } => {}
        }
        timeline.push(now);
    }
    Ok(timeline)
}

fn require_positive(value: Decimal, what: &'static str) -> Result<()> {
    if value <= Decimal::ZERO {
        return Err(Error::NotPositive(what));
    }
    Ok(())
}

fn remove(held: Decimal, units: Decimal) -> Result<Decimal> {
    require_positive(units, "Units disposed")?;
    if units > held {
        return Err(Error::InsufficientUnits {
            requested: units,
            held,
        });
    }
    Ok(held - units)
}

/// P and B of the formula, in units as they were counted on the sale date
/// (a split inside the period would otherwise compare unlike units).
struct Kept {
    acquired: Decimal,
    held_non_registered: Decimal,
    held_registered: Decimal,
}

impl Kept {
    fn held(&self) -> Decimal {
        self.held_non_registered + self.held_registered
    }
}

fn kept_after_loss(
    events: &[Event],
    units: &[Units],
    sale_index: usize,
    rule: &SuperficialLossRules,
) -> Kept {
    let sale_date = events[sale_index].date;
    let sale_factor = units[sale_index].split_factor;
    let start = sale_date - Days::new(u64::from(rule.days_before));
    let end = sale_date + Days::new(u64::from(rule.days_after));

    let acquired = events
        .iter()
        .zip(units)
        .filter(|(event, _)| event.date >= start && event.date <= end)
        .filter_map(|(event, at)| match &event.kind {
            EventKind::Acquire { units, .. } | EventKind::RegisteredAcquire { units } => {
                Some(*units * sale_factor / at.split_factor)
            }
            _ => None,
        })
        .sum();

    // The sale itself is on or before `end`, so a last event always exists.
    let at_end = events
        .iter()
        .zip(units)
        .rfind(|(event, _)| event.date <= end)
        .map_or(units[sale_index], |(_, at)| *at);
    let to_sale_units = sale_factor / at_end.split_factor;

    Kept {
        acquired,
        held_non_registered: at_end.non_registered * to_sale_units,
        held_registered: at_end.registered * to_sale_units,
    }
}

/// One tax year of one ledger, as it goes on a capital gains schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct YearSummary {
    pub year: i32,
    /// Gains, deemed gains included.
    pub gains_cad: Decimal,
    /// Losses that count, after the superficial loss rule (zero or positive).
    pub losses_cad: Decimal,
    /// Losses the rule denied (zero or positive); not part of the net.
    pub denied_losses_cad: Decimal,
    pub net_cad: Decimal,
    /// The net at the year's inclusion rate: a taxable capital gain when
    /// positive, an allowable capital loss when negative.
    pub taxable_cad: Decimal,
    pub provisional: bool,
    pub needs_review: bool,
}

/// `None` when the rule table has no inclusion rate for `year`.
pub fn summarize_year(ledger: &Ledger, year: i32, rules: &Rules) -> Option<YearSummary> {
    let rate = rules.capital_gains_inclusion_rate(year)?;
    let mut summary = YearSummary {
        year,
        gains_cad: Decimal::ZERO,
        losses_cad: Decimal::ZERO,
        denied_losses_cad: Decimal::ZERO,
        net_cad: Decimal::ZERO,
        taxable_cad: Decimal::ZERO,
        provisional: false,
        needs_review: false,
    };
    for record in ledger.dispositions.iter().filter(|r| r.date.year() == year) {
        let reportable = record.reportable_gain_cad();
        if reportable >= Decimal::ZERO {
            summary.gains_cad += reportable;
        } else {
            summary.losses_cad -= reportable;
        }
        summary.denied_losses_cad += record.denied_loss_cad;
        summary.provisional |= record.provisional;
        summary.needs_review |= record.needs_review;
    }
    for deemed in ledger.deemed_gains.iter().filter(|g| g.date.year() == year) {
        summary.gains_cad += deemed.gain_cad;
    }
    summary.net_cad = summary.gains_cad - summary.losses_cad;
    summary.taxable_cad = summary.net_cad * rate;
    Some(summary)
}
