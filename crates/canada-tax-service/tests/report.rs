//! The whole path: accounts and activities as the app stores them, rates
//! from a stand-in Bank of Canada, to the report the interface shows.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::NaiveDate;
use loontax_canada_tax::{Error, RateSource, RateStore, Result, Rules};
use loontax_canada_tax_service::tax_report;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use wealthfolio_core::accounts::Account;
use wealthfolio_core::activities::Activity;

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

#[derive(Default)]
struct Store {
    rates: Mutex<BTreeMap<(String, NaiveDate), Decimal>>,
    coverage: Mutex<BTreeMap<String, (NaiveDate, NaiveDate)>>,
}

#[async_trait]
impl RateStore for Store {
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
        for (date, rate) in rates {
            self.rates
                .lock()
                .unwrap()
                .insert((currency.to_string(), date), rate);
        }
        self.coverage
            .lock()
            .unwrap()
            .insert(currency.to_string(), coverage);
        Ok(())
    }
}

/// Publishes the given USD rates, or fails when it has none.
struct Bank(Vec<(NaiveDate, Decimal)>);

#[async_trait]
impl RateSource for Bank {
    async fn fetch(
        &self,
        _currency: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Decimal)>> {
        if self.0.is_empty() {
            return Err(Error::RateFetch("offline".to_string()));
        }
        Ok(self
            .0
            .iter()
            .filter(|(d, _)| *d >= from && *d <= to)
            .copied()
            .collect())
    }
}

fn account(id: &str, meta: Option<&str>) -> Account {
    Account {
        id: id.to_string(),
        meta: meta.map(str::to_string),
        ..Account::default()
    }
}

fn accounts() -> Vec<Account> {
    vec![
        account(
            "broker-1",
            Some(r#"{"canada":{"registration":"NON_REGISTERED"}}"#),
        ),
        account(
            "broker-2",
            Some(
                r#"{"canada":{"registration":"NON_REGISTERED"},"accounting":{"costBasisMethod":"WAC"}}"#,
            ),
        ),
        account("tfsa", Some(r#"{"canada":{"registration":"TFSA"}}"#)),
        account("new", None),
        account("broken", Some(r#"{"canada":{"registration":"ROTH_IRA"}}"#)),
    ]
}

fn activity(
    id: &str,
    account: &str,
    asset: &str,
    date: &str,
    kind: &str,
    fields: serde_json::Value,
) -> Activity {
    let mut value = json!({
        "id": id, "accountId": account, "assetId": asset, "activityType": kind,
        "status": "POSTED", "activityDate": format!("{date}T15:00:00Z"), "currency": "CAD",
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"
    });
    for (key, field) in fields.as_object().unwrap() {
        value[key] = field.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn history() -> Vec<Activity> {
    let trade = |quantity: &str, amount: &str| json!({"quantity": quantity, "amount": amount});
    let usd = |quantity: &str, amount: &str| json!({"quantity": quantity, "amount": amount, "currency": "USD"});
    vec![
        // XEQT across two brokers: 6,000 over 200 = 30.00; sell 100 for 4,500 -> gain 1,500.
        activity(
            "1",
            "broker-1",
            "XEQT",
            "2026-01-05",
            "BUY",
            trade("100", "2000"),
        ),
        activity(
            "2",
            "broker-2",
            "XEQT",
            "2026-02-02",
            "BUY",
            trade("100", "4000"),
        ),
        activity(
            "3",
            "broker-2",
            "XEQT",
            "2026-06-01",
            "SELL",
            trade("100", "4500"),
        ),
        // VCN: 5,000 for 100, sold for 4,000 (loss 1,000), bought back in the TFSA.
        activity(
            "4",
            "broker-1",
            "VCN",
            "2026-01-05",
            "BUY",
            trade("100", "5000"),
        ),
        activity(
            "5",
            "broker-1",
            "VCN",
            "2026-11-02",
            "SELL",
            trade("100", "4000"),
        ),
        activity(
            "6",
            "tfsa",
            "VCN",
            "2026-11-10",
            "BUY",
            trade("100", "4100"),
        ),
        // VTI in US dollars: US$1,001 at 1.35 = 1,351.35; sold on Canada Day,
        // US$1,099 at June 30's 1.4210 = 1,561.679; gain 210.329.
        activity(
            "7",
            "broker-1",
            "VTI",
            "2026-03-02",
            "BUY",
            usd("10", "1001"),
        ),
        activity(
            "8",
            "broker-1",
            "VTI",
            "2026-07-01",
            "SELL",
            usd("10", "1099"),
        ),
        // ZAG touches an unclassified account; BND one whose settings are unreadable.
        activity("9", "new", "ZAG", "2026-01-05", "BUY", trade("10", "150")),
        activity(
            "10",
            "broken",
            "BND",
            "2026-01-05",
            "BUY",
            trade("10", "700"),
        ),
    ]
}

fn bank() -> Bank {
    Bank(vec![(day(3, 2), dec!(1.35)), (day(6, 30), dec!(1.4210))])
}

#[tokio::test]
async fn the_report_adds_up_and_names_what_it_left_out() {
    let rules = Rules::embedded().unwrap();
    let report = tax_report(
        &accounts(),
        &history(),
        "America/Toronto",
        &rules,
        &Store::default(),
        &bank(),
        day(12, 31),
    )
    .await
    .unwrap();

    assert_eq!(report.holders.len(), 1);
    let holder = &report.holders[0];
    assert_eq!(holder.holder, "self");

    let position = |id: &str| {
        holder
            .positions
            .iter()
            .find(|p| p.security_id == id)
            .unwrap()
    };
    assert_eq!(position("XEQT").acb_total_cad, dec!(3000));
    assert_eq!(position("XEQT").acb_per_unit_cad, Some(dec!(30)));
    assert_eq!(position("XEQT").dispositions[0].record.gain_cad, dec!(1500));
    assert_eq!(
        position("VCN").dispositions[0].record.denied_lost_cad,
        dec!(1000)
    );
    assert_eq!(position("VCN").dispositions[0].reportable_gain_cad, dec!(0));
    assert_eq!(
        position("VTI").dispositions[0].record.gain_cad,
        dec!(210.329)
    );

    // 2026: gains 1,500 + 210.329; the VCN loss is denied in full.
    let year = &holder.years[0];
    assert_eq!(year.year, 2026);
    assert_eq!(year.gains_cad, dec!(1710.329));
    assert_eq!(year.losses_cad, dec!(0));
    assert_eq!(year.denied_losses_cad, dec!(1000));
    assert_eq!(year.net_cad, dec!(1710.329));
    assert_eq!(year.taxable_cad, dec!(855.1645));
    assert!(!year.provisional);

    // The Canada Day sale used June 30's rate, and the report says so.
    assert_eq!(report.fx_fallbacks.len(), 1);
    assert_eq!(report.fx_fallbacks[0].activity_id, "8");
    assert_eq!(report.fx_fallbacks[0].rate_date, day(6, 30));

    // Left out, with the reason.
    let mut left_out: Vec<(&str, &str)> = report
        .issues
        .iter()
        .map(|i| (i.security_id.as_str(), i.code))
        .collect();
    left_out.sort_unstable();
    assert_eq!(
        left_out,
        vec![
            ("BND", "UNCLASSIFIED_ACCOUNT"),
            ("ZAG", "UNCLASSIFIED_ACCOUNT")
        ]
    );
    assert_eq!(report.unreadable_account_ids, vec!["broken".to_string()]);
    assert!(report.rate_failures.is_empty());
}

/// Offline with nothing stored: Canadian-dollar securities are still
/// computed, the US one is left out, and the failed request is reported.
#[tokio::test]
async fn offline_the_report_still_computes_what_it_can() {
    let rules = Rules::embedded().unwrap();
    let report = tax_report(
        &accounts(),
        &history(),
        "America/Toronto",
        &rules,
        &Store::default(),
        &Bank(Vec::new()),
        day(12, 31),
    )
    .await
    .unwrap();

    assert_eq!(report.rate_failures.len(), 1);
    assert_eq!(report.rate_failures[0].currency, "USD");
    assert!(report
        .issues
        .iter()
        .any(|i| i.security_id == "VTI" && i.code == "MISSING_FX_RATE"));
    let year = &report.holders[0].years[0];
    assert_eq!(year.gains_cad, dec!(1500));
}

/// What the interface receives: camelCase keys and stable issue codes.
#[tokio::test]
async fn the_report_serializes_for_the_interface() {
    let rules = Rules::embedded().unwrap();
    let report = tax_report(
        &accounts(),
        &history(),
        "America/Toronto",
        &rules,
        &Store::default(),
        &bank(),
        day(12, 31),
    )
    .await
    .unwrap();
    let value = serde_json::to_value(&report).unwrap();

    assert_eq!(value["asOf"], "2026-12-31");
    let position = &value["holders"][0]["positions"][0];
    assert!(position["acbTotalCad"].is_number());
    assert!(position["dispositions"][0]["netProceedsCad"].is_number());
    assert_eq!(value["issues"][0]["code"], "UNCLASSIFIED_ACCOUNT");
    assert_eq!(value["fxFallbacks"][0]["rateDate"], "2026-06-30");
}
