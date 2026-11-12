//! Hand-worked ACB cases. Expected values are computed by hand from the rule
//! (the comments show the arithmetic), never taken from the code.

use loontax_canada_tax::{to_cad, AcbPool, Error};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Case A: two purchases, one partial sale, commissions in the cost.
#[test]
fn case_a_basic_average_cost() {
    let mut pool = AcbPool::new();

    // Jan 10: 100 x 30 + 5 = 3,005
    pool.acquire(dec!(100), dec!(100) * dec!(30) + dec!(5))
        .unwrap();
    assert_eq!(pool.total_cad(), dec!(3005));
    assert_eq!(pool.per_unit_cad(), Some(dec!(30.05)));

    // Mar 1: 50 x 32 + 5 = 1,605 -> 4,610 over 150 units
    pool.acquire(dec!(50), dec!(50) * dec!(32) + dec!(5))
        .unwrap();
    assert_eq!(pool.total_cad(), dec!(4610));
    assert_eq!(pool.units(), dec!(150));
    assert_eq!(pool.per_unit_cad().unwrap().round_dp(4), dec!(30.7333));

    // Jun 1: sell 60 x 35 - 5 = 2,095; ACB 60 x 4,610 / 150 = 1,844; gain 251
    let sale = pool
        .dispose(dec!(60), dec!(60) * dec!(35) - dec!(5))
        .unwrap();
    assert_eq!(sale.net_proceeds_cad, dec!(2095));
    assert_eq!(sale.acb_relieved_cad, dec!(1844));
    assert_eq!(sale.gain_cad, dec!(251));

    // 90 units keep 4,610 - 1,844 = 2,766, and the average is unchanged.
    assert_eq!(pool.units(), dec!(90));
    assert_eq!(pool.total_cad(), dec!(2766));
    assert_eq!(pool.per_unit_cad().unwrap().round_dp(4), dec!(30.7333));
}

/// Case E: a US-dollar trade converts at each date's own rate.
#[test]
fn case_e_usd_trade_converts_at_each_dates_rate() {
    let mut pool = AcbPool::new();

    // Buy 10 x US$100 + US$1 = US$1,001 at 1.35 -> C$1,351.35
    let cost = to_cad(dec!(10) * dec!(100) + dec!(1), dec!(1.35)).unwrap();
    assert_eq!(cost, dec!(1351.35));
    pool.acquire(dec!(10), cost).unwrap();

    // Sell 10 x US$110 - US$1 = US$1,099 at 1.40 -> C$1,538.60
    let proceeds = to_cad(dec!(10) * dec!(110) - dec!(1), dec!(1.40)).unwrap();
    assert_eq!(proceeds, dec!(1538.60));
    let sale = pool.dispose(dec!(10), proceeds).unwrap();

    // Gain 1,538.60 - 1,351.35 = 187.25. In US dollars the gain is 98, which
    // is 137.20 at 1.40: the other 50.05 is the currency move on the cost.
    assert_eq!(sale.gain_cad, dec!(187.25));
    assert_eq!(pool.units(), dec!(0));
    assert_eq!(pool.total_cad(), dec!(0));
    assert_eq!(pool.per_unit_cad(), None);
}

/// One holder's identical shares pool across accounts: bought at two brokers,
/// sold at one, the cost relieved is the average of both.
#[test]
fn identical_shares_pool_across_accounts() {
    let mut pool = AcbPool::new();
    pool.acquire(dec!(100), dec!(2000)).unwrap(); // broker 1: 20.00 a share
    pool.acquire(dec!(100), dec!(4000)).unwrap(); // broker 2: 40.00 a share

    // Sell broker 2's 100 shares for 4,500. Per account the gain would be 500;
    // the holder's ACB is 30.00 a share, so the gain is 4,500 - 3,000 = 1,500.
    let sale = pool.dispose(dec!(100), dec!(4500)).unwrap();
    assert_eq!(sale.acb_relieved_cad, dec!(3000));
    assert_eq!(sale.gain_cad, dec!(1500));
    assert_eq!(pool.total_cad(), dec!(3000));
}

#[test]
fn a_loss_is_a_negative_gain() {
    let mut pool = AcbPool::new();
    pool.acquire(dec!(100), dec!(5000)).unwrap();
    let sale = pool.dispose(dec!(100), dec!(4000)).unwrap();
    assert_eq!(sale.gain_cad, dec!(-1000));
}

/// Cost is conserved: what sales relieve plus what remains is what was paid,
/// even when the average does not divide evenly.
#[test]
fn cost_is_conserved_when_the_average_does_not_divide_evenly() {
    let mut pool = AcbPool::new();
    pool.acquire(dec!(3), dec!(100)).unwrap();
    let first = pool.dispose(dec!(1), dec!(40)).unwrap();
    let second = pool.dispose(dec!(1), dec!(40)).unwrap();
    assert_eq!(
        first.acb_relieved_cad + second.acb_relieved_cad + pool.total_cad(),
        dec!(100)
    );
    let last = pool.dispose(dec!(1), dec!(40)).unwrap();
    assert_eq!(pool.total_cad(), Decimal::ZERO);
    assert_eq!(
        first.acb_relieved_cad + second.acb_relieved_cad + last.acb_relieved_cad,
        dec!(100)
    );
}

#[test]
fn invalid_trades_are_refused_and_leave_the_pool_unchanged() {
    let mut pool = AcbPool::new();
    pool.acquire(dec!(10), dec!(100)).unwrap();
    let before = pool.clone();

    assert_eq!(
        pool.dispose(dec!(11), dec!(50)),
        Err(Error::InsufficientUnits {
            requested: dec!(11),
            held: dec!(10)
        })
    );
    assert!(matches!(
        pool.dispose(dec!(0), dec!(50)),
        Err(Error::NotPositive(_))
    ));
    assert!(matches!(
        pool.acquire(dec!(-1), dec!(50)),
        Err(Error::NotPositive(_))
    ));
    assert!(matches!(
        pool.acquire(dec!(1), dec!(-50)),
        Err(Error::Negative(_))
    ));
    assert!(matches!(
        to_cad(dec!(1), dec!(0)),
        Err(Error::NotPositive(_))
    ));
    assert_eq!(pool, before);
}
