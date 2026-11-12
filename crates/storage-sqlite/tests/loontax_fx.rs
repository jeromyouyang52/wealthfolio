//! Bank of Canada rates on real SQLite: what is saved is what is read, a
//! second save replaces a rate and the covered range, and currencies are
//! kept apart.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
mod common;

use std::sync::Arc;

use chrono::NaiveDate;
use loontax_canada_tax::{load_rates, FxRates, RateSource, RateStore};
use rust_decimal::Decimal;
use wealthfolio_storage_sqlite::{db, loontax_fx::BocFxRateRepository};

fn rate(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

fn repository() -> (BocFxRateRepository, common::TestDir) {
    let dir = common::TestDir::new();
    let access = db::DbAccess::plaintext(dir.path().join("app.db").to_str().unwrap());
    access.prepare().unwrap();
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let (writer, _task) = db::write_actor::spawn_writer_with_sync_state(
        (*pool).clone(),
        Arc::new(|| {}),
        Arc::default(),
    )
    .unwrap();
    (BocFxRateRepository::new(pool, writer), dir)
}

#[tokio::test]
async fn saved_rates_and_coverage_are_read_back() {
    let (store, _dir) = repository();
    assert_eq!(store.coverage("USD").unwrap(), None);
    assert_eq!(store.rates("USD", day(1, 1), day(12, 31)).unwrap(), vec![]);

    store
        .save(
            "usd",
            vec![(day(6, 29), rate("1.3650")), (day(6, 30), rate("1.3672"))],
            (day(6, 27), day(7, 1)),
        )
        .await
        .unwrap();
    store
        .save(
            "EUR",
            vec![(day(6, 30), rate("1.5601"))],
            (day(6, 30), day(6, 30)),
        )
        .await
        .unwrap();

    assert_eq!(
        store.coverage("USD").unwrap(),
        Some((day(6, 27), day(7, 1)))
    );
    assert_eq!(
        store.rates("USD", day(6, 30), day(12, 31)).unwrap(),
        vec![(day(6, 30), rate("1.3672"))]
    );
    assert_eq!(
        store.rates("EUR", day(1, 1), day(12, 31)).unwrap(),
        vec![(day(6, 30), rate("1.5601"))]
    );

    // A later save corrects a rate and widens the range.
    store
        .save(
            "USD",
            vec![(day(6, 30), rate("1.3675"))],
            (day(6, 1), day(7, 1)),
        )
        .await
        .unwrap();
    assert_eq!(store.coverage("USD").unwrap(), Some((day(6, 1), day(7, 1))));
    assert_eq!(
        store.rates("USD", day(6, 29), day(6, 30)).unwrap(),
        vec![(day(6, 29), rate("1.3650")), (day(6, 30), rate("1.3675"))]
    );
}

struct Bank;

#[async_trait::async_trait]
impl RateSource for Bank {
    async fn fetch(
        &self,
        _currency: &str,
        from: NaiveDate,
        _to: NaiveDate,
    ) -> loontax_canada_tax::Result<Vec<(NaiveDate, Decimal)>> {
        Ok(vec![(from, rate("1.35"))])
    }
}

/// The store behind `load_rates`: fetched once, then served from SQLite.
#[tokio::test]
async fn load_rates_runs_on_the_sqlite_store() {
    let (store, _dir) = repository();
    let needs = [("USD".to_string(), day(3, 10))].into();
    let loaded = load_rates(&needs, &store, &Bank, day(11, 12))
        .await
        .unwrap();

    assert!(loaded.failures.is_empty());
    // The stand-in publishes one rate, on the first day asked (Mar 3): it is
    // within a week of Mar 10, so it stands in for it.
    let quote = loaded.table.quote("USD", day(3, 10)).unwrap();
    assert_eq!(quote.rate_date, day(3, 3));
    assert_eq!(
        store.coverage("USD").unwrap(),
        Some((day(3, 3), day(3, 10)))
    );
}
