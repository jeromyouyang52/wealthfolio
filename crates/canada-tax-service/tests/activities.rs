//! Recorded activities read as the tax ledger's input.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use chrono::{NaiveDate, TimeZone, Utc};
use loontax_canada_tax::TaxActivityKind;
use loontax_canada_tax_service::tax_activities;
use rust_decimal_macros::dec;
use serde_json::json;
use wealthfolio_core::activities::{Activity, ActivityStatus};

/// A posted record on XEQT in `acc-1`, built through the app's own JSON shape.
fn activity(id: &str, activity_type: &str, fields: serde_json::Value) -> Activity {
    let mut value = json!({
        "id": id,
        "accountId": "acc-1",
        "assetId": "XEQT",
        "activityType": activity_type,
        "status": "POSTED",
        "activityDate": "2026-03-02T15:00:00Z",
        "currency": "CAD",
        "createdAt": "2026-03-02T15:00:00Z",
        "updatedAt": "2026-03-02T15:00:00Z"
    });
    for (key, field) in fields.as_object().unwrap() {
        value[key] = field.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn kinds(activities: &[Activity]) -> Vec<TaxActivityKind> {
    tax_activities(activities, "America/Toronto")
        .into_iter()
        .map(|a| a.kind)
        .collect()
}

#[test]
fn trades_carry_units_and_the_final_amount() {
    let read = kinds(&[
        activity(
            "1",
            "BUY",
            json!({"quantity": "100", "amount": "3005", "fee": "5"}),
        ),
        activity(
            "2",
            "SELL",
            json!({"quantity": "60", "amount": "2095", "fee": "5"}),
        ),
        activity("3", "SPLIT", json!({"amount": "2"})),
    ]);
    assert_eq!(
        read,
        vec![
            TaxActivityKind::Buy {
                units: dec!(100),
                cost: dec!(3005)
            },
            TaxActivityKind::Sell {
                units: dec!(60),
                net_proceeds: dec!(2095)
            },
            TaxActivityKind::Split { ratio: dec!(2) },
        ]
    );
}

#[test]
fn dividends_matter_only_when_they_change_the_cost() {
    let read = kinds(&[
        activity("1", "DIVIDEND", json!({"amount": "40"})),
        activity(
            "2",
            "DIVIDEND",
            json!({"subtype": "RETURN_OF_CAPITAL", "amount": "12"}),
        ),
        activity(
            "3",
            "DIVIDEND",
            json!({"subtype": "NOTIONAL_DISTRIBUTION", "amount": "8"}),
        ),
        activity(
            "4",
            "DIVIDEND",
            json!({"subtype": "DRIP", "amount": "100", "quantity": "0.5"}),
        ),
        activity("5", "INTEREST", json!({"amount": "3"})),
        activity("6", "FEE", json!({"amount": "1"})),
    ]);
    assert_eq!(
        read,
        vec![
            TaxActivityKind::ReturnOfCapital { amount: dec!(12) },
            TaxActivityKind::NotionalDistribution { amount: dec!(8) },
            TaxActivityKind::Buy {
                units: dec!(0.5),
                cost: dec!(100)
            },
        ]
    );
}

#[test]
fn what_cannot_be_read_holds_the_security_back_instead_of_vanishing() {
    let draft = {
        let mut a = activity("4", "BUY", json!({"quantity": "1", "amount": "30"}));
        a.status = ActivityStatus::Draft;
        a
    };
    let read = kinds(&[
        activity("1", "BUY", json!({"quantity": "100"})),
        activity(
            "2",
            "SELL",
            json!({"subtype": "POSITION_OPEN", "quantity": "5", "amount": "150"}),
        ),
        activity("3", "ADJUSTMENT", json!({"amount": "10"})),
        draft,
        activity("5", "UNKNOWN", json!({})),
    ]);
    assert_eq!(read, vec![TaxActivityKind::Unreadable; 5]);
}

#[test]
fn void_records_and_cash_records_are_left_out() {
    let void = {
        let mut a = activity("1", "BUY", json!({"quantity": "1", "amount": "30"}));
        a.status = ActivityStatus::Void;
        a
    };
    let cash = activity("2", "DEPOSIT", json!({"assetId": null, "amount": "500"}));
    assert!(kinds(&[void, cash]).is_empty());
}

/// The user's override is the type that counts.
#[test]
fn a_type_override_is_respected() {
    let read = kinds(&[activity(
        "1",
        "UNKNOWN",
        json!({"activityTypeOverride": "BUY", "quantity": "10", "amount": "300"}),
    )]);
    assert_eq!(
        read,
        vec![TaxActivityKind::Buy {
            units: dec!(10),
            cost: dec!(300)
        }]
    );
}

/// 03:30 UTC on March 3 is still March 2 in Toronto; records come out oldest
/// first whatever order they were stored in.
#[test]
fn a_trade_belongs_to_the_users_day_and_order_is_by_time() {
    let mut late = activity("late", "BUY", json!({"quantity": "1", "amount": "30"}));
    late.activity_date = Utc.with_ymd_and_hms(2026, 3, 3, 3, 30, 0).unwrap();
    let early = activity("early", "BUY", json!({"quantity": "1", "amount": "30"}));

    let read = tax_activities(&[late, early], "America/Toronto");
    assert_eq!(read[0].id, "early");
    assert_eq!(read[1].id, "late");
    assert_eq!(read[1].date, NaiveDate::from_ymd_opt(2026, 3, 2).unwrap());
    assert_eq!(read[1].security_id, "XEQT");
    assert_eq!(read[1].account_id, "acc-1");
}

/// An unlinked transfer has no counterparty: the book will hold it back.
#[test]
fn an_unlinked_transfer_has_no_paired_account() {
    let read = kinds(&[activity("1", "TRANSFER_OUT", json!({"quantity": "10"}))]);
    assert_eq!(
        read,
        vec![TaxActivityKind::TransferOut {
            units: dec!(10),
            paired_account_id: None
        }]
    );
}
