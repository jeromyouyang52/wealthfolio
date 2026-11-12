//! The Bank of Canada Valet API as a [`RateSource`].
//!
//! One request asks for one currency's daily rates over a range of days. It
//! carries the currency code and the two dates, and nothing else: no account,
//! holding or amount ever leaves the device. The API is public and needs no key.

use std::time::Duration;

use async_trait::async_trait;
use chrono::NaiveDate;
use loontax_canada_tax::{parse_observations, series_name, Error, RateSource, Result};
use rust_decimal::Decimal;

const VALET_URL: &str = "https://www.bankofcanada.ca/valet";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// A decade of daily rates is under half a megabyte.
const MAX_ANSWER_BYTES: usize = 8 * 1024 * 1024;

pub struct BankOfCanada {
    client: reqwest::Client,
    base_url: String,
}

impl BankOfCanada {
    pub fn new() -> Result<Self> {
        Self::with_base_url(VALET_URL)
    }

    /// For tests, against a local stand-in.
    pub fn with_base_url(base_url: &str) -> Result<Self> {
        let client = wealthfolio_http::client_builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| fetch_error("the HTTP client could not start"))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    fn observations_url(&self, currency: &str, from: NaiveDate, to: NaiveDate) -> Result<String> {
        Ok(format!(
            "{}/observations/{}/json?start_date={}&end_date={}",
            self.base_url,
            series_name(currency)?,
            from.format("%Y-%m-%d"),
            to.format("%Y-%m-%d"),
        ))
    }
}

fn fetch_error(what: &str) -> Error {
    Error::RateFetch(what.to_string())
}

#[async_trait]
impl RateSource for BankOfCanada {
    async fn fetch(
        &self,
        currency: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Decimal)>> {
        let url = self.observations_url(currency, from, to)?;
        let response = self.client.get(url).send().await.map_err(|error| {
            fetch_error(if error.is_timeout() {
                "the Bank of Canada did not answer in time"
            } else {
                "the Bank of Canada could not be reached"
            })
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::RateFetch(format!(
                "the Bank of Canada answered with status {}",
                status.as_u16()
            )));
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| fetch_error("the answer could not be read"))?;
        if body.len() > MAX_ANSWER_BYTES {
            return Err(fetch_error("the answer is larger than expected"));
        }
        let text = std::str::from_utf8(&body).map_err(|_| fetch_error("the answer is not text"))?;
        parse_observations(text, currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_for_one_series_and_two_dates() {
        let bank = BankOfCanada::new().unwrap();
        let day = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        assert_eq!(
            bank.observations_url("usd", day(6, 29), day(7, 3)).unwrap(),
            "https://www.bankofcanada.ca/valet/observations/FXUSDCAD/json?start_date=2026-06-29&end_date=2026-07-03"
        );
        // A code that is not a currency never reaches the URL.
        assert!(bank
            .observations_url("USD/../x", day(6, 29), day(7, 3))
            .is_err());
    }
}
