//! Bank of Canada daily exchange rates.
//!
//! The Bank publishes one rate per currency on each of its business days
//! (series `FX<currency>CAD`, from 2017-01-03). This module reads the Valet
//! API's answer and holds the rates; fetching and storing belong to the
//! caller, so nothing here touches the network.
//!
//! A trade can fall on a day with no rate: US markets open on Canadian
//! holidays. The table then answers with the latest earlier rate, no older
//! than [`MAX_RATE_AGE_DAYS`], and says which day it is from.

use std::collections::BTreeMap;

use chrono::{Days, NaiveDate};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

use crate::book::{FxQuote, FxRates};
use crate::error::{Error, Result};

/// How far back an earlier day's rate may stand in. The Bank's longest
/// closures are a holiday beside a weekend; a week covers them, and a longer
/// gap means rates are missing, not that the Bank was closed.
pub const MAX_RATE_AGE_DAYS: u64 = 7;

/// The first day the Bank's daily rates exist for. Earlier trades need a rate
/// from another source.
pub fn first_rate_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2017, 1, 3).expect("2017-01-03 is a real date")
}

/// The Valet series for a currency, such as `FXUSDCAD`.
pub fn series_name(currency: &str) -> Result<String> {
    let code = currency.trim().to_ascii_uppercase();
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_uppercase()) || code == "CAD" {
        return Err(Error::InvalidRates(
            "a currency is not a three-letter foreign code".to_string(),
        ));
    }
    Ok(format!("FX{code}CAD"))
}

#[derive(Deserialize)]
struct Observations {
    observations: Vec<BTreeMap<String, Value>>,
}

/// Reads the answer of `/valet/observations/<series>/json` for one currency.
/// Days the series has no value for are skipped; anything malformed is an
/// error for the whole answer.
pub fn parse_observations(json: &str, currency: &str) -> Result<Vec<(NaiveDate, Decimal)>> {
    let series = series_name(currency)?;
    let invalid = |what: &str| Error::InvalidRates(what.to_string());
    let parsed: Observations =
        serde_json::from_str(json).map_err(|_| invalid("the answer has no observations"))?;

    let mut rates = Vec::with_capacity(parsed.observations.len());
    for observation in parsed.observations {
        let date = observation
            .get("d")
            .and_then(Value::as_str)
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .ok_or_else(|| invalid("an observation has no date"))?;
        let Some(value) = observation.get(&series).and_then(|entry| entry.get("v")) else {
            continue;
        };
        let rate = value
            .as_str()
            .and_then(|v| v.parse::<Decimal>().ok())
            .filter(|rate| *rate > Decimal::ZERO)
            .ok_or_else(|| invalid("a rate is not a positive number"))?;
        rates.push((date, rate));
    }
    Ok(rates)
}

/// The first and last day to ask the Bank for so that every day in `dates`
/// can be answered, earlier-day fallback included. `None` for no dates.
pub fn fetch_range(dates: impl IntoIterator<Item = NaiveDate>) -> Option<(NaiveDate, NaiveDate)> {
    let mut range: Option<(NaiveDate, NaiveDate)> = None;
    for date in dates {
        range = Some(match range {
            None => (date, date),
            Some((first, last)) => (first.min(date), last.max(date)),
        });
    }
    range.map(|(first, last)| (first - Days::new(MAX_RATE_AGE_DAYS), last))
}

/// Published rates by currency and day.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RateTable {
    rates: BTreeMap<String, BTreeMap<NaiveDate, Decimal>>,
}

impl RateTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, currency: &str, date: NaiveDate, cad_per_unit: Decimal) {
        self.rates
            .entry(currency.trim().to_ascii_uppercase())
            .or_default()
            .insert(date, cad_per_unit);
    }

    pub fn extend(
        &mut self,
        currency: &str,
        rates: impl IntoIterator<Item = (NaiveDate, Decimal)>,
    ) {
        for (date, rate) in rates {
            self.insert(currency, date, rate);
        }
    }
}

impl FxRates for RateTable {
    fn quote(&self, currency: &str, date: NaiveDate) -> Option<FxQuote> {
        let oldest = date - Days::new(MAX_RATE_AGE_DAYS);
        self.rates
            .get(&currency.trim().to_ascii_uppercase())?
            .range(oldest..=date)
            .next_back()
            .map(|(rate_date, cad_per_unit)| FxQuote {
                cad_per_unit: *cad_per_unit,
                rate_date: *rate_date,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("the cases use real dates")
    }

    /// The shape the Valet API documents: one object per day, the series as a key.
    const ANSWER: &str = r#"{
        "terms": {"url": "https://www.bankofcanada.ca/terms/"},
        "seriesDetail": {"FXUSDCAD": {"label": "USD/CAD", "description": "US dollar to Canadian dollar daily exchange rate"}},
        "observations": [
            {"d": "2026-06-29", "FXUSDCAD": {"v": "1.3650"}},
            {"d": "2026-06-30", "FXUSDCAD": {"v": "1.3672"}},
            {"d": "2026-07-02", "FXUSDCAD": {"v": "1.3701"}},
            {"d": "2026-07-03"}
        ]
    }"#;

    #[test]
    fn series_names() {
        assert_eq!(series_name("usd").as_deref(), Ok("FXUSDCAD"));
        assert_eq!(series_name(" EUR ").as_deref(), Ok("FXEURCAD"));
        for bad in ["CAD", "US", "USDT", "U$D", ""] {
            assert!(series_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_the_valet_answer_and_skips_days_without_a_value() {
        let rates = parse_observations(ANSWER, "USD").unwrap();
        assert_eq!(
            rates,
            vec![
                (day(2026, 6, 29), dec!(1.3650)),
                (day(2026, 6, 30), dec!(1.3672)),
                (day(2026, 7, 2), dec!(1.3701)),
            ]
        );
        // The same answer holds nothing for another currency.
        assert_eq!(parse_observations(ANSWER, "EUR").unwrap(), vec![]);
    }

    #[test]
    fn a_malformed_answer_is_refused_whole() {
        for bad in [
            r#"{"message": "Series FXXXXCAD not found."}"#,
            r#"{"observations": [{"FXUSDCAD": {"v": "1.36"}}]}"#,
            r#"{"observations": [{"d": "2026-13-40", "FXUSDCAD": {"v": "1.36"}}]}"#,
            r#"{"observations": [{"d": "2026-06-29", "FXUSDCAD": {"v": "n/a"}}]}"#,
            r#"{"observations": [{"d": "2026-06-29", "FXUSDCAD": {"v": "0"}}]}"#,
            r#"{"observations": [{"d": "2026-06-29", "FXUSDCAD": {"v": 1.36}}]}"#,
            "not json",
        ] {
            assert!(
                matches!(parse_observations(bad, "USD"), Err(Error::InvalidRates(_))),
                "{bad}"
            );
        }
    }

    /// Canada Day 2026 is a Wednesday: no rate on July 1, so June 30's stands in.
    #[test]
    fn a_day_without_a_rate_takes_the_latest_earlier_one() {
        let mut table = RateTable::new();
        table.extend("USD", parse_observations(ANSWER, "USD").unwrap());

        let same_day = table.quote("usd", day(2026, 6, 30)).unwrap();
        assert_eq!(same_day.cad_per_unit, dec!(1.3672));
        assert_eq!(same_day.rate_date, day(2026, 6, 30));

        let holiday = table.quote("USD", day(2026, 7, 1)).unwrap();
        assert_eq!(holiday.cad_per_unit, dec!(1.3672));
        assert_eq!(holiday.rate_date, day(2026, 6, 30));

        // Never a later day's rate, and never one older than a week.
        assert_eq!(table.quote("USD", day(2026, 6, 28)), None);
        assert!(table.quote("USD", day(2026, 7, 9)).is_some());
        assert_eq!(table.quote("USD", day(2026, 7, 10)), None);
        assert_eq!(table.quote("EUR", day(2026, 6, 30)), None);
    }

    #[test]
    fn the_range_to_fetch_reaches_back_for_the_fallback() {
        assert_eq!(fetch_range([]), None);
        assert_eq!(
            fetch_range([day(2026, 7, 1), day(2026, 3, 10), day(2026, 5, 5)]),
            Some((day(2026, 3, 3), day(2026, 7, 1)))
        );
    }
}
