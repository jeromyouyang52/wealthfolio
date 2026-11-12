//! SQLite storage for the Bank of Canada rates of the Canadian tax ledger
//! (`loontax-canada-tax`).
//!
//! The tables are declared here, not in `schema.rs`: that file is generated
//! from upstream's schema, and these two tables are read by nothing else.

use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use loontax_canada_tax::{Error as TaxError, RateStore, Result as TaxResult};
use rust_decimal::Decimal;

use crate::db::{get_connection, WriteHandle};
use crate::errors::StorageError;

diesel::table! {
    loontax_boc_fx_rates (currency, rate_date) {
        currency -> Text,
        rate_date -> Text,
        cad_per_unit -> Text,
    }
}

diesel::table! {
    loontax_boc_fx_coverage (currency) {
        currency -> Text,
        covered_from -> Text,
        covered_to -> Text,
    }
}

const DAY_FORMAT: &str = "%Y-%m-%d";

pub struct BocFxRateRepository {
    pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    writer: WriteHandle,
}

impl BocFxRateRepository {
    pub fn new(pool: Arc<Pool<ConnectionManager<SqliteConnection>>>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}

// Messages name what failed and never carry stored values.
fn store_error(what: &str) -> TaxError {
    TaxError::RateStore(what.to_string())
}

fn key(currency: &str) -> String {
    currency.trim().to_ascii_uppercase()
}

fn text(day: NaiveDate) -> String {
    day.format(DAY_FORMAT).to_string()
}

fn day(text: &str) -> TaxResult<NaiveDate> {
    NaiveDate::parse_from_str(text, DAY_FORMAT).map_err(|_| store_error("a stored day is invalid"))
}

#[async_trait]
impl RateStore for BocFxRateRepository {
    fn coverage(&self, currency: &str) -> TaxResult<Option<(NaiveDate, NaiveDate)>> {
        use loontax_boc_fx_coverage::dsl;
        let mut conn =
            get_connection(&self.pool).map_err(|_| store_error("no database connection"))?;
        let row = dsl::loontax_boc_fx_coverage
            .find(key(currency))
            .select((dsl::covered_from, dsl::covered_to))
            .first::<(String, String)>(&mut conn)
            .optional()
            .map_err(|_| store_error("the covered range could not be read"))?;
        row.map(|(from, to)| Ok((day(&from)?, day(&to)?)))
            .transpose()
    }

    fn rates(
        &self,
        currency: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> TaxResult<Vec<(NaiveDate, Decimal)>> {
        use loontax_boc_fx_rates::dsl;
        let mut conn =
            get_connection(&self.pool).map_err(|_| store_error("no database connection"))?;
        // ISO days sort as text.
        let rows = dsl::loontax_boc_fx_rates
            .filter(dsl::currency.eq(key(currency)))
            .filter(dsl::rate_date.ge(text(from)))
            .filter(dsl::rate_date.le(text(to)))
            .order(dsl::rate_date.asc())
            .select((dsl::rate_date, dsl::cad_per_unit))
            .load::<(String, String)>(&mut conn)
            .map_err(|_| store_error("the rates could not be read"))?;
        rows.into_iter()
            .map(|(date, rate)| {
                let rate = Decimal::from_str(&rate)
                    .map_err(|_| store_error("a stored rate is invalid"))?;
                Ok((day(&date)?, rate))
            })
            .collect()
    }

    async fn save(
        &self,
        currency: &str,
        rates: Vec<(NaiveDate, Decimal)>,
        coverage: (NaiveDate, NaiveDate),
    ) -> TaxResult<()> {
        let currency = key(currency);
        self.writer
            .exec_tx(move |tx| -> wealthfolio_core::Result<()> {
                for (date, rate) in rates {
                    diesel::replace_into(loontax_boc_fx_rates::table)
                        .values((
                            loontax_boc_fx_rates::currency.eq(&currency),
                            loontax_boc_fx_rates::rate_date.eq(text(date)),
                            loontax_boc_fx_rates::cad_per_unit.eq(rate.to_string()),
                        ))
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                }
                diesel::replace_into(loontax_boc_fx_coverage::table)
                    .values((
                        loontax_boc_fx_coverage::currency.eq(&currency),
                        loontax_boc_fx_coverage::covered_from.eq(text(coverage.0)),
                        loontax_boc_fx_coverage::covered_to.eq(text(coverage.1)),
                    ))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
            .map_err(|_| store_error("the rates could not be saved"))
    }
}
