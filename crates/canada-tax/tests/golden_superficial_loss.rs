//! Hand-worked superficial loss cases. Expected values are computed by hand
//! from the CRA formula (least of S, P, B) / S x L, never taken from the code.

use chrono::NaiveDate;
use loontax_canada_tax::{summarize_year, Event, EventKind, Ledger, Rules};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).expect("the cases use real dates")
}

fn buy(date: NaiveDate, units: Decimal, cost_cad: Decimal) -> Event {
    Event {
        date,
        kind: EventKind::Acquire { units, cost_cad },
    }
}

fn sell(date: NaiveDate, units: Decimal, net_proceeds_cad: Decimal) -> Event {
    Event {
        date,
        kind: EventKind::Dispose {
            units,
            net_proceeds_cad,
        },
    }
}

fn registered_buy(date: NaiveDate, units: Decimal) -> Event {
    Event {
        date,
        kind: EventKind::RegisteredAcquire { units },
    }
}

fn compute(events: Vec<Event>) -> Ledger {
    let rules = Rules::embedded().expect("the embedded table loads");
    Ledger::compute(events, &rules.superficial_loss, day(12, 31))
        .expect("the case's history is valid")
}

/// 100 shares that cost 5,000, sold Nov 1 for 4,000: a 1,000 loss.
fn hundred_sold_at_a_loss() -> Vec<Event> {
    vec![
        buy(day(1, 5), dec!(100), dec!(5000)),
        sell(day(11, 1), dec!(100), dec!(4000)),
    ]
}

/// Case B: all 100 bought back on Nov 20 and kept.
#[test]
fn case_b_full_repurchase_denies_the_whole_loss() {
    let mut events = hundred_sold_at_a_loss();
    events.push(buy(day(11, 20), dec!(100), dec!(4100)));
    let ledger = compute(events);

    // S = 100, P = 100, B = 100: 100/100 x 1,000 = 1,000 denied.
    let sale = &ledger.dispositions[0];
    assert_eq!(sale.gain_cad, dec!(-1000));
    assert_eq!(sale.denied_loss_cad, dec!(1000));
    assert_eq!(sale.denied_added_to_acb_cad, dec!(1000));
    assert_eq!(sale.denied_lost_cad, dec!(0));
    assert_eq!(sale.reportable_gain_cad(), dec!(0));
    assert!(!sale.provisional && !sale.needs_review);

    // The new shares cost 4,100 + 1,000.
    assert_eq!(ledger.pool.units(), dec!(100));
    assert_eq!(ledger.pool.total_cad(), dec!(5100));
}

/// Case C: only 40 bought back.
#[test]
fn case_c_partial_repurchase_denies_a_share_of_the_loss() {
    let mut events = hundred_sold_at_a_loss();
    events.push(buy(day(11, 20), dec!(40), dec!(1640)));
    let ledger = compute(events);

    // least(100, 40, 40) / 100 x 1,000 = 400 denied, 600 allowed.
    let sale = &ledger.dispositions[0];
    assert_eq!(sale.denied_loss_cad, dec!(400));
    assert_eq!(sale.reportable_gain_cad(), dec!(-600));

    // 40 x 41 + 400 = 2,040.
    assert_eq!(ledger.pool.units(), dec!(40));
    assert_eq!(ledger.pool.total_cad(), dec!(2040));
}

/// Case D: bought back inside a TFSA. The loss is denied and has no cost to
/// go to.
#[test]
fn case_d_repurchase_in_a_registered_plan_loses_the_loss() {
    let mut events = hundred_sold_at_a_loss();
    events.push(registered_buy(day(11, 20), dec!(100)));
    let ledger = compute(events);

    let sale = &ledger.dispositions[0];
    assert_eq!(sale.denied_loss_cad, dec!(1000));
    assert_eq!(sale.denied_added_to_acb_cad, dec!(0));
    assert_eq!(sale.denied_lost_cad, dec!(1000));
    assert_eq!(sale.reportable_gain_cad(), dec!(0));
    assert!(!sale.needs_review);

    assert_eq!(ledger.pool.units(), dec!(0));
    assert_eq!(ledger.pool.total_cad(), dec!(0));
}

#[test]
fn no_repurchase_allows_the_loss() {
    let ledger = compute(hundred_sold_at_a_loss());
    let sale = &ledger.dispositions[0];
    assert_eq!(sale.denied_loss_cad, dec!(0));
    assert_eq!(sale.reportable_gain_cad(), dec!(-1000));
}

/// Nov 1 + 30 days is Dec 1: a purchase that day is inside the period, one
/// on Dec 2 is outside. Before the sale, Oct 2 is inside and Oct 1 outside.
#[test]
fn the_period_is_30_days_each_side_inclusive() {
    for (date, denied) in [(day(12, 1), dec!(1000)), (day(12, 2), dec!(0))] {
        let mut events = hundred_sold_at_a_loss();
        events.push(buy(date, dec!(100), dec!(4100)));
        assert_eq!(
            compute(events).dispositions[0].denied_loss_cad,
            denied,
            "{date}"
        );
    }

    // 100 old shares at 50.00, then 100 more bought before the sale at 42.00:
    // 9,200 over 200 = 46.00. Selling 100 for 4,000 loses 600.
    for (date, denied) in [(day(10, 2), dec!(600)), (day(10, 1), dec!(0))] {
        let ledger = compute(vec![
            buy(day(1, 5), dec!(100), dec!(5000)),
            buy(date, dec!(100), dec!(4200)),
            sell(day(11, 1), dec!(100), dec!(4000)),
        ]);
        assert_eq!(ledger.dispositions[0].gain_cad, dec!(-600));
        assert_eq!(ledger.dispositions[0].denied_loss_cad, denied, "{date}");
    }
}

/// A purchase before the sale: the denied loss goes straight into the shares
/// still held.
#[test]
fn purchase_before_the_sale_carries_the_loss_into_the_shares_kept() {
    let ledger = compute(vec![
        buy(day(1, 5), dec!(100), dec!(5000)),
        buy(day(10, 15), dec!(100), dec!(4200)),
        sell(day(11, 1), dec!(100), dec!(4000)),
    ]);
    // S = 100, P = 100, B = 100: the whole 600 is denied.
    // Kept: 9,200 - 4,600 relieved + 600 = 5,200 over 100 shares.
    assert_eq!(ledger.dispositions[0].denied_loss_cad, dec!(600));
    assert_eq!(ledger.pool.units(), dec!(100));
    assert_eq!(ledger.pool.total_cad(), dec!(5200));
}

/// Bought back, then sold again before the period ends: nothing is owned at
/// the end, so the first loss stands.
#[test]
fn shares_not_owned_at_the_end_of_the_period_deny_nothing() {
    let mut events = hundred_sold_at_a_loss();
    events.push(buy(day(11, 10), dec!(100), dec!(4100)));
    events.push(sell(day(11, 15), dec!(100), dec!(4300)));
    let ledger = compute(events);

    assert_eq!(ledger.dispositions[0].denied_loss_cad, dec!(0));
    assert_eq!(ledger.dispositions[0].reportable_gain_cad(), dec!(-1000));
    // The second sale: 4,300 - 4,100 = 200 gain, with no carried loss in its cost.
    assert_eq!(ledger.dispositions[1].gain_cad, dec!(200));
}

#[test]
fn a_gain_is_never_superficial() {
    let ledger = compute(vec![
        buy(day(1, 5), dec!(100), dec!(5000)),
        sell(day(11, 1), dec!(100), dec!(6000)),
        buy(day(11, 2), dec!(100), dec!(6000)),
    ]);
    assert_eq!(ledger.dispositions[0].denied_loss_cad, dec!(0));
    assert_eq!(ledger.dispositions[0].reportable_gain_cad(), dec!(1000));
    assert_eq!(ledger.pool.total_cad(), dec!(6000));
}

/// A 2-for-1 split inside the period: 200 new shares replace the 100 sold.
#[test]
fn a_split_inside_the_period_compares_like_units() {
    let mut events = hundred_sold_at_a_loss();
    events.push(Event {
        date: day(11, 10),
        kind: EventKind::Split { ratio: dec!(2) },
    });
    events.push(buy(day(11, 20), dec!(200), dec!(4100)));
    let ledger = compute(events);

    // In sale-date units P = B = 200 / 2 = 100: the whole loss is denied.
    assert_eq!(ledger.dispositions[0].denied_loss_cad, dec!(1000));
    assert_eq!(ledger.pool.units(), dec!(200));
    assert_eq!(ledger.pool.total_cad(), dec!(5100));
}

/// 50 bought back outside and 50 inside a registered plan: the denied loss
/// follows where the shares sit, and the record asks for review.
#[test]
fn shares_kept_inside_and_outside_registered_plans_are_flagged() {
    let mut events = hundred_sold_at_a_loss();
    events.push(buy(day(11, 20), dec!(50), dec!(2050)));
    events.push(registered_buy(day(11, 21), dec!(50)));
    let ledger = compute(events);

    let sale = &ledger.dispositions[0];
    assert_eq!(sale.denied_loss_cad, dec!(1000));
    assert_eq!(sale.denied_added_to_acb_cad, dec!(500));
    assert_eq!(sale.denied_lost_cad, dec!(500));
    assert!(sale.needs_review);
    assert_eq!(ledger.pool.total_cad(), dec!(2550));
}

/// A loss sold less than 30 days before `as_of` can still be denied.
#[test]
fn a_recent_loss_is_provisional() {
    let rules = Rules::embedded().unwrap();
    let ledger = Ledger::compute(
        hundred_sold_at_a_loss(),
        &rules.superficial_loss,
        day(11, 30),
    )
    .unwrap();
    assert!(ledger.dispositions[0].provisional);

    let settled = Ledger::compute(
        hundred_sold_at_a_loss(),
        &rules.superficial_loss,
        day(12, 1),
    )
    .unwrap();
    assert!(!settled.dispositions[0].provisional);
}

/// Returns of capital reduce the cost; beyond it they are a gain. A
/// reinvested distribution raises the cost.
#[test]
fn return_of_capital_and_reinvested_distribution() {
    let event = |date, kind| Event { date, kind };
    let ledger = compute(vec![
        buy(day(1, 5), dec!(100), dec!(1000)),
        // 1,000 + 150 = 1,150
        event(
            day(3, 31),
            EventKind::NotionalDistribution {
                amount_cad: dec!(150),
            },
        ),
        // 1,150 - 200 = 950
        event(
            day(6, 30),
            EventKind::ReturnOfCapital {
                amount_cad: dec!(200),
            },
        ),
        // 950 - 1,000: the cost goes to 0 and 50 is a capital gain
        event(
            day(9, 30),
            EventKind::ReturnOfCapital {
                amount_cad: dec!(1000),
            },
        ),
    ]);
    assert_eq!(ledger.pool.total_cad(), dec!(0));
    assert_eq!(ledger.pool.units(), dec!(100));
    assert_eq!(ledger.deemed_gains.len(), 1);
    assert_eq!(ledger.deemed_gains[0].gain_cad, dec!(50));
}

/// The year's schedule: a gain, an allowed loss, a denied loss, a deemed gain.
#[test]
fn year_summary_nets_gains_and_allowed_losses() {
    let rules = Rules::embedded().unwrap();
    let ledger = compute(vec![
        buy(day(1, 5), dec!(300), dec!(15000)),  // 50.00 a share
        sell(day(2, 2), dec!(100), dec!(5800)),  // gain 800
        sell(day(5, 4), dec!(100), dec!(4700)),  // loss 300, allowed
        sell(day(11, 1), dec!(100), dec!(4000)), // loss 1,000
        buy(day(11, 20), dec!(40), dec!(1640)),  // denies 400 of it
    ]);
    let summary = summarize_year(&ledger, 2026, &rules).unwrap();

    assert_eq!(summary.gains_cad, dec!(800));
    assert_eq!(summary.losses_cad, dec!(900)); // 300 + 600
    assert_eq!(summary.denied_losses_cad, dec!(400));
    assert_eq!(summary.net_cad, dec!(-100));
    assert_eq!(summary.taxable_cad, dec!(-50)); // one half
    assert!(!summary.provisional && !summary.needs_review);

    assert_eq!(
        summarize_year(&ledger, 2025, &rules).unwrap().net_cad,
        dec!(0)
    );
    assert_eq!(summarize_year(&ledger, 1999, &rules), None);
}

#[test]
fn selling_more_than_held_is_refused() {
    let rules = Rules::embedded().unwrap();
    let result = Ledger::compute(
        vec![
            buy(day(1, 5), dec!(10), dec!(100)),
            sell(day(2, 1), dec!(11), dec!(100)),
        ],
        &rules.superficial_loss,
        day(12, 31),
    );
    assert!(result.is_err());
}
