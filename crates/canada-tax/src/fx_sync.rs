//! Keeping the Bank of Canada rates a calculation needs.
//!
//! Rates are stored once fetched, so a calculation works offline and asks the
//! Bank only for days it has not asked about. What is sent is a currency code
//! and a date range, nothing about holdings or amounts.
//!
//! Each currency keeps one covered range of days: every rate the Bank
//! published inside it is stored, so a day in it without a rate is a day the
//! Bank published none. The range stops at yesterday, because today's rate
//! may not be out yet.
//!
//! A failed fetch never fails the calculation: stored rates are still used,
//! the failure is returned beside them, and a security whose rate is missing
//! is reported as such by the book.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{Days, NaiveDate};
use rust_decimal::Decimal;

use crate::error::{Error, Result};
use crate::fx::{fetch_range, first_rate_day, RateTable};

/// Where fetched rates are kept.
#[async_trait]
pub trait RateStore: Send + Sync {
    /// The first and last day already asked about for a currency.
    fn coverage(&self, currency: &str) -> Result<Option<(NaiveDate, NaiveDate)>>;

    fn rates(
        &self,
        currency: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Decimal)>>;

    /// Stores rates and the currency's new covered range together.
    async fn save(
        &self,
        currency: &str,
        rates: Vec<(NaiveDate, Decimal)>,
        coverage: (NaiveDate, NaiveDate),
    ) -> Result<()>;
}

/// The Bank of Canada, or a stand-in for it.
#[async_trait]
pub trait RateSource: Send + Sync {
    /// Every rate published for `currency` from `from` to `to`, both included.
    async fn fetch(
        &self,
        currency: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Decimal)>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchFailure {
    pub currency: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub error: Error,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadedRates {
    pub table: RateTable,
    pub failures: Vec<FetchFailure>,
}

/// Returns the stored rates for `needs` (currency and day pairs), after
/// fetching the days not yet covered. `today` is the user's current day.
pub async fn load_rates(
    needs: &BTreeSet<(String, NaiveDate)>,
    store: &dyn RateStore,
    source: &dyn RateSource,
    today: NaiveDate,
) -> Result<LoadedRates> {
    let mut days: BTreeMap<&str, Vec<NaiveDate>> = BTreeMap::new();
    for (currency, date) in needs {
        days.entry(currency).or_default().push(*date);
    }

    let mut loaded = LoadedRates::default();
    for (currency, dates) in days {
        let Some((first, last)) = fetch_range(dates) else {
            continue;
        };
        let wanted = (first.max(first_rate_day()), last.min(today - Days::new(1)));
        let mut coverage = store.coverage(currency)?;
        for (from, to) in uncovered(wanted, coverage) {
            match source.fetch(currency, from, to).await {
                Ok(rates) => {
                    let grown = match coverage {
                        None => (from, to),
                        Some((start, end)) => (start.min(from), end.max(to)),
                    };
                    store.save(currency, rates, grown).await?;
                    coverage = Some(grown);
                }
                Err(error) => loaded.failures.push(FetchFailure {
                    currency: currency.to_string(),
                    from,
                    to,
                    error,
                }),
            }
        }
        loaded
            .table
            .extend(currency, store.rates(currency, first, last)?);
    }
    Ok(loaded)
}

/// The parts of `wanted` outside `coverage`, kept adjacent to it so the
/// covered range stays one range.
fn uncovered(
    wanted: (NaiveDate, NaiveDate),
    coverage: Option<(NaiveDate, NaiveDate)>,
) -> Vec<(NaiveDate, NaiveDate)> {
    let (from, to) = wanted;
    if from > to {
        return Vec::new();
    }
    let Some((start, end)) = coverage else {
        return vec![(from, to)];
    };
    let mut parts = Vec::new();
    if from < start {
        parts.push((from, start - Days::new(1)));
    }
    if to > end {
        parts.push((end + Days::new(1), to));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::FxRates;
    use rust_decimal_macros::dec;
    use std::sync::Mutex;

    fn day(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).expect("the cases use real dates")
    }

    #[derive(Default)]
    struct MemoryStore {
        rates: Mutex<BTreeMap<(String, NaiveDate), Decimal>>,
        coverage: Mutex<BTreeMap<String, (NaiveDate, NaiveDate)>>,
    }

    #[async_trait]
    impl RateStore for MemoryStore {
        fn coverage(&self, currency: &str) -> Result<Option<(NaiveDate, NaiveDate)>> {
            Ok(self.coverage.lock().unwrap().get(currency).copied())
        }

        fn rates(
            &self,
            currency: &str,
            from: NaiveDate,
            to: NaiveDate,
        ) -> Result<Vec<(NaiveDate, Decimal)>> {
            Ok(self
                .rates
                .lock()
                .unwrap()
                .iter()
                .filter(|((c, d), _)| c == currency && *d >= from && *d <= to)
                .map(|((_, d), r)| (*d, *r))
                .collect())
        }

        async fn save(
            &self,
            currency: &str,
            rates: Vec<(NaiveDate, Decimal)>,
            coverage: (NaiveDate, NaiveDate),
        ) -> Result<()> {
            let mut stored = self.rates.lock().unwrap();
            for (date, rate) in rates {
                stored.insert((currency.to_string(), date), rate);
            }
            self.coverage
                .lock()
                .unwrap()
                .insert(currency.to_string(), coverage);
            Ok(())
        }
    }

    /// Publishes 1.35 on weekdays, and records what it was asked.
    #[derive(Default)]
    struct Bank {
        asked: Mutex<Vec<(String, NaiveDate, NaiveDate)>>,
        down: bool,
    }

    #[async_trait]
    impl RateSource for Bank {
        async fn fetch(
            &self,
            currency: &str,
            from: NaiveDate,
            to: NaiveDate,
        ) -> Result<Vec<(NaiveDate, Decimal)>> {
            self.asked
                .lock()
                .unwrap()
                .push((currency.to_string(), from, to));
            if self.down {
                return Err(Error::RateFetch("offline".to_string()));
            }
            Ok(from
                .iter_days()
                .take_while(|d| *d <= to)
                .filter(|d| chrono::Datelike::weekday(d).number_from_monday() <= 5)
                .map(|d| (d, dec!(1.35)))
                .collect())
        }
    }

    fn needs(dates: &[NaiveDate]) -> BTreeSet<(String, NaiveDate)> {
        dates.iter().map(|d| ("USD".to_string(), *d)).collect()
    }

    #[tokio::test]
    async fn fetches_once_then_answers_from_the_store() {
        let (store, bank) = (MemoryStore::default(), Bank::default());
        let today = day(11, 12);

        // Needs Mar 10 to Jun 1: asks from a week before the first day.
        let loaded = load_rates(&needs(&[day(3, 10), day(6, 1)]), &store, &bank, today)
            .await
            .unwrap();
        assert!(loaded.failures.is_empty());
        assert_eq!(
            *bank.asked.lock().unwrap(),
            vec![("USD".to_string(), day(3, 3), day(6, 1))]
        );
        assert!(loaded.table.quote("USD", day(3, 10)).is_some());

        // The same needs again, and a day inside the range: nothing is asked.
        load_rates(&needs(&[day(3, 10), day(4, 15)]), &store, &bank, today)
            .await
            .unwrap();
        assert_eq!(bank.asked.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn asks_only_for_the_days_outside_the_covered_range() {
        let (store, bank) = (MemoryStore::default(), Bank::default());
        let today = day(11, 12);
        load_rates(&needs(&[day(3, 10), day(6, 1)]), &store, &bank, today)
            .await
            .unwrap();

        load_rates(&needs(&[day(2, 2), day(9, 1)]), &store, &bank, today)
            .await
            .unwrap();
        let asked = bank.asked.lock().unwrap();
        assert_eq!(asked[1], ("USD".to_string(), day(1, 26), day(3, 2)));
        assert_eq!(asked[2], ("USD".to_string(), day(6, 2), day(9, 1)));
        assert_eq!(
            store.coverage("USD").unwrap(),
            Some((day(1, 26), day(9, 1)))
        );
    }

    /// Today's rate may not be out: today is never marked as covered, and a
    /// trade of today is asked about again tomorrow.
    #[tokio::test]
    async fn today_is_not_covered() {
        let (store, bank) = (MemoryStore::default(), Bank::default());
        load_rates(&needs(&[day(11, 12)]), &store, &bank, day(11, 12))
            .await
            .unwrap();
        assert_eq!(
            store.coverage("USD").unwrap(),
            Some((day(11, 5), day(11, 11)))
        );

        load_rates(&needs(&[day(11, 12)]), &store, &bank, day(11, 13))
            .await
            .unwrap();
        assert_eq!(
            bank.asked.lock().unwrap()[1],
            ("USD".to_string(), day(11, 12), day(11, 12))
        );
    }

    #[tokio::test]
    async fn a_failed_fetch_keeps_the_stored_rates_and_reports_the_failure() {
        let store = MemoryStore::default();
        load_rates(&needs(&[day(3, 10)]), &store, &Bank::default(), day(11, 12))
            .await
            .unwrap();

        let offline = Bank {
            down: true,
            ..Bank::default()
        };
        let loaded = load_rates(
            &needs(&[day(3, 10), day(6, 1)]),
            &store,
            &offline,
            day(11, 12),
        )
        .await
        .unwrap();

        assert_eq!(loaded.failures.len(), 1);
        assert_eq!(loaded.failures[0].from, day(3, 11));
        assert!(loaded.table.quote("USD", day(3, 10)).is_some());
        assert_eq!(loaded.table.quote("USD", day(6, 1)), None);
        // The failed days stay uncovered, to be asked about next time.
        assert_eq!(
            store.coverage("USD").unwrap(),
            Some((day(3, 3), day(3, 10)))
        );
    }

    /// The daily series starts on 2017-01-03: earlier days are never asked for.
    #[tokio::test]
    async fn days_before_the_series_are_not_asked_for() {
        let (store, bank) = (MemoryStore::default(), Bank::default());
        let old = NaiveDate::from_ymd_opt(2015, 6, 1).unwrap();
        let loaded = load_rates(
            &[("USD".to_string(), old)].into(),
            &store,
            &bank,
            day(11, 12),
        )
        .await
        .unwrap();
        assert!(bank.asked.lock().unwrap().is_empty());
        assert_eq!(loaded.table.quote("USD", old), None);
    }
}
