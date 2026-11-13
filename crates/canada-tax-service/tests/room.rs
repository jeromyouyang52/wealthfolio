//! Contribution room from accounts and activities as the app stores them,
//! with rates from a stand-in Bank of Canada.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::NaiveDate;
use loontax_canada_tax::{Error, RateSource, RateStore, Result, Rules};
use loontax_canada_tax_service::contribution_room;
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
        account("tfsa", Some(r#"{"canada":{"registration":"TFSA"}}"#)),
        account("tfsa-2", Some(r#"{"canada":{"registration":"TFSA"}}"#)),
        account(
            "cash",
            Some(r#"{"canada":{"registration":"NON_REGISTERED"}}"#),
        ),
        account("rrsp", Some(r#"{"canada":{"registration":"RRSP"}}"#)),
        account(
            "spousal",
            Some(r#"{"canada":{"registration":"SPOUSAL_RRSP","holder":"spouse"}}"#),
        ),
        account("fhsa", Some(r#"{"canada":{"registration":"FHSA"}}"#)),
        account("unknown", None),
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

fn money(id: &str, account: &str, date: &str, kind: &str, fields: serde_json::Value) -> Activity {
    let mut record = activity(id, account, "", date, kind, fields);
    record.asset_id = None;
    record
}

fn history() -> Vec<Activity> {
    let amount = |a: &str| json!({ "amount": a });
    vec![
        money("t1", "tfsa", "2026-02-02", "DEPOSIT", amount("5000")),
        money(
            "t2",
            "tfsa",
            "2026-03-02",
            "DEPOSIT",
            json!({"amount": "1000", "currency": "USD"}),
        ),
        money("t3", "tfsa", "2026-04-01", "WITHDRAWAL", amount("500")),
        // Shares moved from the holder's non-registered account: a contribution.
        activity(
            "t4-out",
            "cash",
            "XEQT",
            "2026-05-04",
            "TRANSFER_OUT",
            json!({"quantity": "10", "unitPrice": "50", "sourceGroupId": "g1"}),
        ),
        activity(
            "t4-in",
            "tfsa",
            "XEQT",
            "2026-05-04",
            "TRANSFER_IN",
            json!({"quantity": "10", "unitPrice": "50", "sourceGroupId": "g1"}),
        ),
        // Between the holder's own TFSAs: no room used.
        money(
            "t5-out",
            "tfsa",
            "2026-06-01",
            "TRANSFER_OUT",
            json!({"amount": "300", "sourceGroupId": "g2"}),
        ),
        money(
            "t5-in",
            "tfsa-2",
            "2026-06-01",
            "TRANSFER_IN",
            json!({"amount": "300", "sourceGroupId": "g2"}),
        ),
        // From another institution.
        money("t6", "tfsa", "2026-06-15", "TRANSFER_IN", amount("2000")),
        money(
            "t7",
            "tfsa",
            "2026-07-01",
            "DEPOSIT",
            json!({"amount": "100", "status": "PENDING"}),
        ),
        money("t8", "tfsa", "2026-07-02", "DIVIDEND", amount("40")),
        money("r1", "rrsp", "2026-04-01", "DEPOSIT", amount("6000")),
        money("r2", "rrsp", "2026-08-01", "WITHDRAWAL", amount("1000")),
        money("s1", "spousal", "2026-04-01", "DEPOSIT", amount("2000")),
        money(
            "f1-out",
            "rrsp",
            "2026-09-01",
            "TRANSFER_OUT",
            json!({"amount": "3000", "sourceGroupId": "g3"}),
        ),
        money(
            "f1-in",
            "fhsa",
            "2026-09-01",
            "TRANSFER_IN",
            json!({"amount": "3000", "sourceGroupId": "g3"}),
        ),
        money("f2", "fhsa", "2026-09-15", "DEPOSIT", amount("4000")),
        money("u1", "unknown", "2026-09-15", "DEPOSIT", amount("999")),
    ]
}

const PROFILES: &str = r#"{
    "self": {
        "tfsa": {"basis": "craRoom", "year": 2026, "room": "10000"},
        "rrsp": {"year": 2026, "availableRoom": "20000"},
        "fhsa": {"openedYear": 2026}
    }
}"#;

#[tokio::test]
async fn room_follows_the_recorded_money() {
    let rules = Rules::embedded().unwrap();
    let bank = Bank(vec![(day(3, 2), dec!(1.40))]);
    let report = contribution_room(
        &accounts(),
        &history(),
        Some(PROFILES),
        "America/Toronto",
        &rules,
        &Store::default(),
        &bank,
        day(11, 13),
    )
    .await
    .unwrap();

    assert!(!report.unreadable_profiles);
    let holders: Vec<&str> = report.holders.iter().map(|h| h.holder.as_str()).collect();
    assert_eq!(holders, ["self", "spouse"]);
    let me = &report.holders[0];

    let tfsa = me.tfsa.as_ref().unwrap();
    assert_eq!(tfsa.status, "ok");
    assert!(tfsa.incomplete, "the pending deposit is left out");
    let room = tfsa.room.as_ref().unwrap();
    // 5,000 + 1,000 USD at 1.40 + 500 of shares.
    assert_eq!(room.contributions_cad, dec!(6900));
    assert_eq!(room.withdrawals_cad, dec!(500));
    assert_eq!(room.available_cad, dec!(3100));

    let rrsp = me.rrsp.as_ref().unwrap().room.as_ref().unwrap();
    assert_eq!(rrsp.contributions_cad, dec!(6000));
    assert_eq!(rrsp.remaining_cad, dec!(14000));

    let fhsa = me.fhsa.as_ref().unwrap().room.as_ref().unwrap();
    assert_eq!(fhsa.rrsp_transfers_cad, dec!(3000));
    assert_eq!(fhsa.contributions_cad, dec!(4000));
    assert_eq!(fhsa.remaining_cad, dec!(1000));

    // The spouse has a spousal RRSP but entered no figure.
    let spouse = &report.holders[1];
    let spousal = spouse.rrsp.as_ref().unwrap();
    assert_eq!(spousal.status, "needsStart");
    assert!(spousal.has_account);
    assert!(spouse.tfsa.is_none());

    let codes: Vec<(&str, &str)> = report
        .issues
        .iter()
        .map(|issue| (issue.code, issue.activity_id.as_deref().unwrap()))
        .collect();
    assert_eq!(
        codes,
        [
            ("SPOUSAL_RRSP_CONTRIBUTION", "s1"),
            ("ASSUMED_DIRECT_TRANSFER", "t6"),
            ("UNCONFIRMED_ACTIVITY", "t7"),
        ]
    );
    assert!(report.fx_fallbacks.is_empty());
}

#[tokio::test]
async fn a_missing_rate_leaves_the_flow_out_and_says_so() {
    let rules = Rules::embedded().unwrap();
    let report = contribution_room(
        &accounts(),
        &history(),
        Some(PROFILES),
        "America/Toronto",
        &rules,
        &Store::default(),
        &Bank(Vec::new()),
        day(11, 13),
    )
    .await
    .unwrap();
    assert_eq!(report.rate_failures.len(), 1);
    let tfsa = report.holders[0].tfsa.as_ref().unwrap();
    assert_eq!(tfsa.room.as_ref().unwrap().contributions_cad, dec!(5500));
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "MISSING_FX_RATE"
                && issue.activity_id.as_deref() == Some("t2"))
    );
}

#[tokio::test]
async fn unreadable_profiles_ask_for_the_figures_again() {
    let rules = Rules::embedded().unwrap();
    let report = contribution_room(
        &accounts(),
        &[],
        Some("{not json"),
        "America/Toronto",
        &rules,
        &Store::default(),
        &Bank(Vec::new()),
        day(11, 13),
    )
    .await
    .unwrap();
    assert!(report.unreadable_profiles);
    assert_eq!(
        report.holders[0].tfsa.as_ref().unwrap().status,
        "needsStart"
    );
    assert!(
        report.rate_failures.is_empty(),
        "no rate is needed, none is asked for"
    );
}
