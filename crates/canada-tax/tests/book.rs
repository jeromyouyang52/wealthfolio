//! From activities across accounts to ledgers. Expected values are worked by
//! hand in the comments.

use std::collections::HashMap;

use chrono::NaiveDate;
use loontax_canada_tax::{
    CanadaAccountInfo, FxQuote, FxRates, IssueReason, RateTable, Registration, Rules, TaxActivity,
    TaxActivityKind, TaxBook,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).expect("the cases use real dates")
}

struct Rates(Vec<(NaiveDate, Decimal)>);

impl FxRates for Rates {
    fn quote(&self, currency: &str, date: NaiveDate) -> Option<FxQuote> {
        if currency != "USD" {
            return None;
        }
        let (rate_date, cad_per_unit) = self.0.iter().find(|(d, _)| *d == date)?;
        Some(FxQuote {
            cad_per_unit: *cad_per_unit,
            rate_date: *rate_date,
        })
    }
}

fn account(registration: Registration, holder: &str) -> Option<CanadaAccountInfo> {
    Some(CanadaAccountInfo {
        registration,
        holder: holder.to_string(),
    })
}

/// Two taxable brokers and a TFSA of one holder, a spouse's account, and an
/// account nobody classified.
fn accounts() -> HashMap<String, Option<CanadaAccountInfo>> {
    HashMap::from([
        (
            "broker-1".to_string(),
            account(Registration::NonRegistered, "self"),
        ),
        (
            "broker-2".to_string(),
            account(Registration::NonRegistered, "self"),
        ),
        ("tfsa".to_string(), account(Registration::Tfsa, "self")),
        (
            "spouse".to_string(),
            account(Registration::NonRegistered, "spouse"),
        ),
        ("new".to_string(), None),
    ])
}

fn activity(account: &str, security: &str, date: NaiveDate, kind: TaxActivityKind) -> TaxActivity {
    TaxActivity {
        id: format!("{account}-{security}-{date}"),
        account_id: account.to_string(),
        security_id: security.to_string(),
        date,
        currency: "CAD".to_string(),
        kind,
    }
}

fn buy(units: Decimal, cost: Decimal) -> TaxActivityKind {
    TaxActivityKind::Buy { units, cost }
}

fn sell(units: Decimal, net_proceeds: Decimal) -> TaxActivityKind {
    TaxActivityKind::Sell {
        units,
        net_proceeds,
    }
}

fn build(activities: &[TaxActivity], rates: Rates) -> TaxBook {
    let rules = Rules::embedded().expect("the embedded table loads");
    TaxBook::build(
        activities,
        &accounts(),
        &rates,
        &rules.superficial_loss,
        day(12, 31),
    )
}

fn no_rates() -> Rates {
    Rates(Vec::new())
}

/// Bought at two brokers, sold at one: 6,000 over 200 is 30.00 a share, so
/// the sale of 100 for 4,500 gains 1,500 (the broker's own books say 500).
#[test]
fn one_holders_taxable_accounts_share_a_pool() {
    let book = build(
        &[
            activity("broker-1", "XEQT", day(1, 5), buy(dec!(100), dec!(2000))),
            activity("broker-2", "XEQT", day(2, 2), buy(dec!(100), dec!(4000))),
            activity("broker-2", "XEQT", day(6, 1), sell(dec!(100), dec!(4500))),
        ],
        no_rates(),
    );
    assert!(book.issues.is_empty());
    assert_eq!(book.positions.len(), 1);
    let ledger = &book.positions[0].ledger;
    assert_eq!(ledger.dispositions[0].gain_cad, dec!(1500));
    assert_eq!(ledger.pool.total_cad(), dec!(3000));
}

/// Case D through accounts: sold at a loss outside, bought back in the TFSA.
#[test]
fn a_repurchase_in_the_holders_tfsa_denies_the_loss_for_good() {
    let book = build(
        &[
            activity("broker-1", "XEQT", day(1, 5), buy(dec!(100), dec!(5000))),
            activity("broker-1", "XEQT", day(11, 1), sell(dec!(100), dec!(4000))),
            activity("tfsa", "XEQT", day(11, 20), buy(dec!(100), dec!(4100))),
        ],
        no_rates(),
    );
    let sale = &book.positions[0].ledger.dispositions[0];
    assert_eq!(sale.denied_loss_cad, dec!(1000));
    assert_eq!(sale.denied_lost_cad, dec!(1000));
    // The TFSA purchase is no part of the pool.
    assert_eq!(book.positions[0].ledger.pool.units(), dec!(0));
}

/// Holders do not pool: 10.00 a share for one, 30.00 for the other. Selling
/// 100 for 2,000 gains 1,000 for the first. (A spouse's repurchase is not yet
/// tested against the superficial loss rule.)
#[test]
fn each_holder_has_their_own_pool() {
    let book = build(
        &[
            activity("broker-1", "XEQT", day(1, 5), buy(dec!(100), dec!(1000))),
            activity("spouse", "XEQT", day(1, 6), buy(dec!(100), dec!(3000))),
            activity("broker-1", "XEQT", day(6, 1), sell(dec!(100), dec!(2000))),
        ],
        no_rates(),
    );
    assert_eq!(book.positions.len(), 2);
    let own = book
        .positions
        .iter()
        .find(|p| p.holder == "self")
        .expect("self");
    let spouse = book
        .positions
        .iter()
        .find(|p| p.holder == "spouse")
        .expect("spouse");
    assert_eq!(own.ledger.dispositions[0].gain_cad, dec!(1000));
    assert_eq!(spouse.ledger.pool.total_cad(), dec!(3000));
}

/// Case E through accounts: US$1,001 at 1.35 is C$1,351.35; US$1,099 at 1.40
/// is C$1,538.60; the gain is 187.25.
#[test]
fn foreign_trades_convert_at_their_own_days_rate() {
    let usd = |mut a: TaxActivity| {
        a.currency = "USD".to_string();
        a
    };
    let trades = [
        usd(activity(
            "broker-1",
            "VTI",
            day(3, 2),
            buy(dec!(10), dec!(1001)),
        )),
        usd(activity(
            "broker-1",
            "VTI",
            day(9, 1),
            sell(dec!(10), dec!(1099)),
        )),
    ];
    let book = build(
        &trades,
        Rates(vec![(day(3, 2), dec!(1.35)), (day(9, 1), dec!(1.40))]),
    );
    assert_eq!(
        book.positions[0].ledger.dispositions[0].gain_cad,
        dec!(187.25)
    );

    // Without the sale day's rate the security is left out, not estimated.
    let book = build(&trades, Rates(vec![(day(3, 2), dec!(1.35))]));
    assert!(book.positions.is_empty());
    assert_eq!(
        book.issues[0].reason,
        IssueReason::MissingFxRate {
            currency: "USD".to_string(),
            date: day(9, 1)
        }
    );
}

/// A security traded in an unclassified account is left out for everyone;
/// other securities are still computed.
#[test]
fn an_unclassified_account_holds_back_only_its_securities() {
    let book = build(
        &[
            activity("broker-1", "XEQT", day(1, 5), buy(dec!(100), dec!(2000))),
            activity("new", "XEQT", day(2, 2), buy(dec!(100), dec!(4000))),
            activity("broker-1", "VCN", day(1, 5), buy(dec!(10), dec!(400))),
        ],
        no_rates(),
    );
    assert_eq!(book.positions.len(), 1);
    assert_eq!(book.positions[0].security_id, "VCN");
    assert_eq!(book.issues.len(), 1);
    assert_eq!(book.issues[0].security_id, "XEQT");
    assert_eq!(
        book.issues[0].reason,
        IssueReason::UnclassifiedAccount {
            account_id: "new".to_string()
        }
    );
    assert!(book.securities_with_issues().contains("XEQT"));
}

/// Shares moved between the holder's two taxable brokers stay in the pool at
/// the same cost: 100 for 2,000, sold for 2,600, gains 600.
#[test]
fn a_transfer_between_the_holders_taxable_accounts_changes_nothing() {
    let book = build(
        &[
            activity("broker-1", "XEQT", day(1, 5), buy(dec!(100), dec!(2000))),
            activity(
                "broker-1",
                "XEQT",
                day(3, 2),
                TaxActivityKind::TransferOut {
                    units: dec!(100),
                    paired_account_id: Some("broker-2".to_string()),
                },
            ),
            activity(
                "broker-2",
                "XEQT",
                day(3, 4),
                TaxActivityKind::TransferIn {
                    units: dec!(100),
                    paired_account_id: Some("broker-1".to_string()),
                },
            ),
            activity("broker-2", "XEQT", day(6, 1), sell(dec!(100), dec!(2600))),
        ],
        no_rates(),
    );
    assert!(book.issues.is_empty());
    assert_eq!(book.positions[0].ledger.dispositions[0].gain_cad, dec!(600));
}

/// A transfer into the TFSA is a deemed sale, and one from nowhere has no
/// cost: neither is guessed.
#[test]
fn other_transfers_hold_the_security_back() {
    for paired in [Some("tfsa".to_string()), None] {
        let book = build(
            &[
                activity("broker-1", "XEQT", day(1, 5), buy(dec!(100), dec!(2000))),
                activity(
                    "broker-1",
                    "XEQT",
                    day(3, 2),
                    TaxActivityKind::TransferOut {
                        units: dec!(100),
                        paired_account_id: paired,
                    },
                ),
            ],
            no_rates(),
        );
        assert!(book.positions.is_empty());
        assert!(matches!(
            book.issues[0].reason,
            IssueReason::UnsupportedTransfer { .. }
        ));
    }
}

/// Each account records the split, a day apart: it is one split. 100 taxable
/// shares become 200, not 400.
#[test]
fn a_split_recorded_on_two_accounts_counts_once() {
    let split = |ratio| TaxActivityKind::Split { ratio };
    let history = |tfsa_ratio| {
        vec![
            activity("broker-1", "XEQT", day(1, 5), buy(dec!(100), dec!(1000))),
            activity("tfsa", "XEQT", day(1, 6), buy(dec!(50), dec!(500))),
            activity("broker-1", "XEQT", day(3, 2), split(dec!(2))),
            activity("tfsa", "XEQT", day(3, 3), split(tfsa_ratio)),
        ]
    };
    let book = build(&history(dec!(2)), no_rates());
    assert_eq!(book.positions[0].ledger.pool.units(), dec!(200));
    assert_eq!(book.positions[0].ledger.pool.total_cad(), dec!(1000));

    let book = build(&history(dec!(3)), no_rates());
    assert!(book.positions.is_empty());
    assert_eq!(
        book.issues[0].reason,
        IssueReason::ConflictingSplits { date: day(3, 3) }
    );
}

/// A sale of more than was ever bought is an issue on that security only.
#[test]
fn a_history_that_does_not_add_up_is_an_issue() {
    let book = build(
        &[
            activity("broker-1", "XEQT", day(1, 5), buy(dec!(10), dec!(100))),
            activity("broker-1", "XEQT", day(2, 2), sell(dec!(11), dec!(120))),
            activity("broker-1", "VCN", day(1, 5), buy(dec!(10), dec!(400))),
        ],
        no_rates(),
    );
    assert_eq!(book.positions.len(), 1);
    assert!(matches!(
        book.issues[0].reason,
        IssueReason::InvalidHistory(_)
    ));
}

/// Sold on Canada Day, when the Bank of Canada publishes no rate: June 30's
/// rate converts the sale, and the book says so.
/// Cost US$1,001 at 1.35 = 1,351.35; proceeds US$1,099 at 1.3672 = 1,502.5528;
/// gain 151.2028.
#[test]
fn a_trade_on_a_day_without_a_rate_is_converted_and_reported() {
    let usd = |mut a: TaxActivity| {
        a.currency = "USD".to_string();
        a
    };
    let trades = [
        usd(activity(
            "broker-1",
            "VTI",
            day(3, 2),
            buy(dec!(10), dec!(1001)),
        )),
        usd(activity(
            "broker-1",
            "VTI",
            day(7, 1),
            sell(dec!(10), dec!(1099)),
        )),
        usd(activity("tfsa", "VTI", day(7, 1), buy(dec!(5), dec!(550)))),
    ];
    assert_eq!(
        TaxBook::fx_needs(&trades, &accounts()),
        [
            ("USD".to_string(), day(3, 2)),
            ("USD".to_string(), day(7, 1))
        ]
        .into()
    );

    let mut table = RateTable::new();
    table.insert("USD", day(3, 2), dec!(1.35));
    table.insert("USD", day(6, 30), dec!(1.3672));
    let rules = Rules::embedded().expect("the embedded table loads");
    let book = TaxBook::build(
        &trades,
        &accounts(),
        &table,
        &rules.superficial_loss,
        day(12, 31),
    );

    assert!(book.issues.is_empty());
    assert_eq!(
        book.positions[0].ledger.dispositions[0].gain_cad,
        dec!(151.2028)
    );
    assert_eq!(book.fx_fallbacks.len(), 1);
    assert_eq!(book.fx_fallbacks[0].date, day(7, 1));
    assert_eq!(book.fx_fallbacks[0].rate_date, day(6, 30));
}
