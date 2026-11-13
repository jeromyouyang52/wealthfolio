//! The tax report the app shows: every holder's adjusted cost bases and
//! capital gains by year, and everything that could not be computed.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, NaiveDate};
use loontax_canada_tax::{
    load_rates, summarize_year, CanadaAccountInfo, DeemedGain, DispositionRecord, FxFallback,
    IssueReason, RateSource, RateStore, Result, Rules, TaxBook, YearSummary,
};
use rust_decimal::Decimal;
use serde::Serialize;
use wealthfolio_core::accounts::Account;
use wealthfolio_core::activities::Activity;

use crate::activities::tax_activities;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxReport {
    pub as_of: NaiveDate,
    pub holders: Vec<HolderReport>,
    /// Securities left out, and why. Their figures are in no total.
    pub issues: Vec<IssueReport>,
    /// Conversions made with an earlier day's Bank of Canada rate.
    pub fx_fallbacks: Vec<FxFallback>,
    /// Rate requests to the Bank of Canada that failed this time.
    pub rate_failures: Vec<RateFailure>,
    /// Accounts whose stored registration could not be read; they count as
    /// unclassified.
    pub unreadable_account_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderReport {
    pub holder: String,
    /// One entry per year with a disposition or deemed gain, oldest first.
    pub years: Vec<YearSummary>,
    /// Years with gains or losses but no inclusion rate in the rule table.
    pub years_without_rate: Vec<i32>,
    pub positions: Vec<PositionReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionReport {
    pub security_id: String,
    pub units: Decimal,
    pub acb_total_cad: Decimal,
    pub acb_per_unit_cad: Option<Decimal>,
    pub dispositions: Vec<DispositionRecord>,
    pub deemed_gains: Vec<DeemedGain>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueReport {
    pub security_id: String,
    pub holder: Option<String>,
    /// A stable code for the interface to translate.
    pub code: &'static str,
    pub account_id: Option<String>,
    pub activity_id: Option<String>,
    pub currency: Option<String>,
    pub date: Option<NaiveDate>,
    /// For `INVALID_HISTORY`, what does not add up.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateFailure {
    pub currency: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub detail: String,
}

/// Computes the report from everything recorded. Fetches the Bank of Canada
/// rates not stored yet; a failed fetch is reported and does not fail the
/// report. Fails only when stored rates cannot be read or written.
pub async fn tax_report(
    accounts: &[Account],
    activities: &[Activity],
    timezone: &str,
    rules: &Rules,
    store: &dyn RateStore,
    source: &dyn RateSource,
    today: NaiveDate,
) -> Result<TaxReport> {
    let mut registrations: HashMap<String, Option<CanadaAccountInfo>> = HashMap::new();
    let mut unreadable_account_ids = Vec::new();
    for account in accounts {
        let info = CanadaAccountInfo::from_meta(account.meta.as_deref()).unwrap_or_else(|_| {
            unreadable_account_ids.push(account.id.clone());
            None
        });
        registrations.insert(account.id.clone(), info);
    }

    let recorded = tax_activities(activities, timezone);
    let needs = TaxBook::fx_needs(&recorded, &registrations);
    let rates = load_rates(&needs, store, source, today).await?;
    let book = TaxBook::build(
        &recorded,
        &registrations,
        &rates.table,
        &rules.superficial_loss,
        today,
    );

    let mut holders: BTreeMap<String, HolderReport> = BTreeMap::new();
    for position in &book.positions {
        let holder = holders
            .entry(position.holder.clone())
            .or_insert_with(|| HolderReport {
                holder: position.holder.clone(),
                years: Vec::new(),
                years_without_rate: Vec::new(),
                positions: Vec::new(),
            });
        let ledger = &position.ledger;
        let years: BTreeSet<i32> = ledger
            .dispositions
            .iter()
            .map(|record| record.date.year())
            .chain(ledger.deemed_gains.iter().map(|gain| gain.date.year()))
            .collect();
        for year in years {
            match summarize_year(ledger, year, rules) {
                Some(summary) => add_year(&mut holder.years, summary),
                None if holder.years_without_rate.contains(&year) => {}
                None => holder.years_without_rate.push(year),
            }
        }
        holder.positions.push(PositionReport {
            security_id: position.security_id.clone(),
            units: ledger.pool.units(),
            acb_total_cad: ledger.pool.total_cad(),
            acb_per_unit_cad: ledger.pool.per_unit_cad(),
            dispositions: ledger.dispositions.clone(),
            deemed_gains: ledger.deemed_gains.clone(),
        });
    }
    let mut holders: Vec<HolderReport> = holders.into_values().collect();
    for holder in &mut holders {
        holder.years.sort_by_key(|summary| summary.year);
        holder.years_without_rate.sort_unstable();
    }

    Ok(TaxReport {
        as_of: today,
        holders,
        issues: book.issues.iter().map(issue_report).collect(),
        fx_fallbacks: book.fx_fallbacks,
        rate_failures: rates
            .failures
            .into_iter()
            .map(|failure| RateFailure {
                currency: failure.currency,
                from: failure.from,
                to: failure.to,
                detail: failure.error.to_string(),
            })
            .collect(),
        unreadable_account_ids,
    })
}

/// Adds one security's year to the holder's. The taxable amount adds up
/// because a year has one inclusion rate.
fn add_year(years: &mut Vec<YearSummary>, summary: YearSummary) {
    match years.iter_mut().find(|year| year.year == summary.year) {
        None => years.push(summary),
        Some(total) => {
            total.gains_cad += summary.gains_cad;
            total.losses_cad += summary.losses_cad;
            total.denied_losses_cad += summary.denied_losses_cad;
            total.net_cad += summary.net_cad;
            total.taxable_cad += summary.taxable_cad;
            total.provisional |= summary.provisional;
            total.needs_review |= summary.needs_review;
        }
    }
}

fn issue_report(issue: &loontax_canada_tax::Issue) -> IssueReport {
    let mut report = IssueReport {
        security_id: issue.security_id.clone(),
        holder: issue.holder.clone(),
        code: "",
        account_id: None,
        activity_id: None,
        currency: None,
        date: None,
        detail: None,
    };
    match &issue.reason {
        IssueReason::UnclassifiedAccount { account_id } => {
            report.code = "UNCLASSIFIED_ACCOUNT";
            report.account_id = Some(account_id.clone());
        }
        IssueReason::MissingFxRate { currency, date } => {
            report.code = "MISSING_FX_RATE";
            report.currency = Some(currency.clone());
            report.date = Some(*date);
        }
        IssueReason::UnsupportedTransfer { activity_id } => {
            report.code = "UNSUPPORTED_TRANSFER";
            report.activity_id = Some(activity_id.clone());
        }
        IssueReason::UnreadableActivity { activity_id } => {
            report.code = "UNREADABLE_ACTIVITY";
            report.activity_id = Some(activity_id.clone());
        }
        IssueReason::ConflictingSplits { date } => {
            report.code = "CONFLICTING_SPLITS";
            report.date = Some(*date);
        }
        IssueReason::InvalidHistory(error) => {
            report.code = "INVALID_HISTORY";
            report.detail = Some(error.to_string());
        }
    }
    report
}
