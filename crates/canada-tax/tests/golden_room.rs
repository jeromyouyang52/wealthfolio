//! Contribution room cases worked by hand from the definitions in subsection
//! 207.01(1) of the Income Tax Act and the CRA's FHSA pages.

use chrono::NaiveDate;
use loontax_canada_tax::{
    fhsa_room, rrsp_room, tfsa_room, FhsaStart, FlowKind, HolderProfile, PlanFlow, RrspStart,
    Rules, TfsaStart,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("the cases use real dates")
}

fn flow(date: NaiveDate, kind: FlowKind, amount: Decimal) -> PlanFlow {
    PlanFlow {
        activity_id: format!("{date}-{amount}"),
        date,
        kind,
        amount_cad: amount,
    }
}

fn put(date: NaiveDate, amount: Decimal) -> PlanFlow {
    flow(date, FlowKind::Contribution, amount)
}

fn take(date: NaiveDate, amount: Decimal) -> PlanFlow {
    flow(date, FlowKind::Withdrawal, amount)
}

fn rules() -> Rules {
    Rules::embedded().expect("the shipped rule table is valid")
}

fn months(excess: &[loontax_canada_tax::ExcessMonth]) -> Vec<(NaiveDate, Decimal)> {
    excess
        .iter()
        .map(|m| (m.month, m.highest_excess_cad))
        .collect()
}

#[test]
fn tfsa_withdrawal_does_not_restore_room_the_same_year() {
    let start = TfsaStart::CraRoom {
        year: 2026,
        room: dec!(20000),
    };
    let flows = [
        put(day(2026, 8, 15), dec!(8000)),
        put(day(2026, 2, 10), dec!(15000)),
        take(day(2026, 5, 1), dec!(5000)),
    ];
    let room = tfsa_room(&start, &flows, &rules(), day(2026, 11, 13)).expect("computes");

    assert_eq!(room.contributions_cad, dec!(23000));
    assert_eq!(room.withdrawals_cad, dec!(5000));
    assert_eq!(room.available_cad, dec!(-3000));
    assert_eq!(room.excess_cad, dec!(3000));
    // August to November, 1% of $3,000 each.
    assert_eq!(
        months(&room.years[0].excess_months),
        vec![
            (day(2026, 8, 1), dec!(3000)),
            (day(2026, 9, 1), dec!(3000)),
            (day(2026, 10, 1), dec!(3000)),
            (day(2026, 11, 1), dec!(3000)),
        ]
    );
    assert_eq!(room.estimated_tax_cad, dec!(120));
    // No 2027 limit in the table yet.
    assert_eq!(room.next_year_room_cad, None);
}

#[test]
fn tfsa_withdrawing_the_excess_stops_the_tax_after_that_month() {
    let start = TfsaStart::CraRoom {
        year: 2026,
        room: dec!(20000),
    };
    let flows = [
        put(day(2026, 2, 10), dec!(15000)),
        take(day(2026, 5, 1), dec!(5000)),
        put(day(2026, 8, 15), dec!(8000)),
        take(day(2026, 9, 20), dec!(3000)),
    ];
    let room = tfsa_room(&start, &flows, &rules(), day(2026, 11, 13)).expect("computes");

    assert_eq!(room.available_cad, dec!(0));
    assert_eq!(room.excess_cad, dec!(0));
    assert_eq!(room.withdrawals_cad, dec!(8000));
    assert_eq!(
        months(&room.years[0].excess_months),
        vec![(day(2026, 8, 1), dec!(3000)), (day(2026, 9, 1), dec!(3000))]
    );
    assert_eq!(room.estimated_tax_cad, dec!(60));
}

#[test]
fn tfsa_estimate_from_eligibility_adds_withdrawals_back_next_year() {
    let start = TfsaStart::EligibleSince { year: 2024 };
    let flows = [
        put(day(2024, 3, 1), dec!(7000)),
        take(day(2025, 6, 1), dec!(2000)),
    ];
    let room = tfsa_room(&start, &flows, &rules(), day(2026, 3, 1)).expect("computes");

    assert_eq!(room.basis, "eligibleSince");
    let jan1: Vec<_> = room
        .years
        .iter()
        .map(|y| (y.year, y.room_jan1_cad))
        .collect();
    assert_eq!(
        jan1,
        vec![(2024, dec!(7000)), (2025, dec!(7000)), (2026, dec!(16000))]
    );
    assert_eq!(room.available_cad, dec!(16000));
}

#[test]
fn tfsa_estimate_starts_in_2009_for_holders_eligible_earlier() {
    let start = TfsaStart::EligibleSince { year: 1990 };
    let room = tfsa_room(&start, &[], &rules(), day(2026, 1, 2)).expect("computes");
    // The CRA's cumulative room for someone eligible since 2009.
    assert_eq!(room.room_jan1_cad, dec!(109000));
}

#[test]
fn tfsa_excess_carried_into_january_is_absorbed_by_new_room() {
    let start = TfsaStart::CraRoom {
        year: 2025,
        room: dec!(1000),
    };
    let small = tfsa_room(
        &start,
        &[put(day(2025, 11, 15), dec!(3000))],
        &rules(),
        day(2026, 2, 10),
    )
    .expect("computes");
    assert_eq!(small.room_jan1_cad, dec!(5000));
    assert_eq!(small.excess_cad, dec!(0));
    assert_eq!(small.estimated_tax_cad, dec!(40));

    let large = tfsa_room(
        &start,
        &[put(day(2025, 11, 15), dec!(10000))],
        &rules(),
        day(2026, 2, 10),
    )
    .expect("computes");
    assert_eq!(large.room_jan1_cad, dec!(-2000));
    assert_eq!(large.excess_cad, dec!(2000));
    // November and December at $9,000, January and February at $2,000.
    assert_eq!(large.estimated_tax_cad, dec!(220));
    assert_eq!(large.flows_before_start, 0);
}

#[test]
fn tfsa_flows_before_the_cra_figure_are_left_out() {
    let start = TfsaStart::CraRoom {
        year: 2026,
        room: dec!(7000),
    };
    let flows = [put(day(2025, 12, 30), dec!(5000))];
    let room = tfsa_room(&start, &flows, &rules(), day(2026, 4, 1)).expect("computes");
    assert_eq!(room.available_cad, dec!(7000));
    assert_eq!(room.flows_before_start, 1);
}

#[test]
fn rrsp_counts_the_window_from_day_61_to_day_60_next_year() {
    let start = RrspStart {
        year: 2026,
        available_room: dec!(10000),
    };
    let flows = [
        put(day(2026, 2, 15), dec!(3000)),
        put(day(2026, 3, 2), dec!(9000)),
        put(day(2026, 10, 5), dec!(4000)),
        put(day(2027, 2, 20), dec!(1000)),
        put(day(2027, 3, 5), dec!(500)),
        take(day(2026, 6, 1), dec!(2500)),
    ];
    let room = rrsp_room(&start, &flows, &rules(), day(2027, 4, 1)).expect("computes");

    assert_eq!(room.window_start, day(2026, 3, 2));
    assert_eq!(room.window_end, day(2027, 3, 1));
    assert_eq!(room.contributions_cad, dec!(14000));
    assert_eq!(room.remaining_cad, dec!(-4000));
    assert_eq!(room.before_window_cad, dec!(3000));
    assert_eq!(room.after_window_cad, dec!(500));
    assert!(room.outdated);
    // $1,000 over the $2,000 cushion from October to December; January's
    // new room is not known here.
    assert_eq!(
        months(&room.excess_months),
        vec![
            (day(2026, 10, 1), dec!(1000)),
            (day(2026, 11, 1), dec!(1000)),
            (day(2026, 12, 1), dec!(1000)),
        ]
    );
    assert_eq!(room.estimated_tax_cad, dec!(30));
    assert_eq!(room.next_year_dollar_limit_cad, Some(dec!(35390)));
}

#[test]
fn rrsp_window_in_a_leap_year_starts_on_march_1() {
    let start = RrspStart {
        year: 2028,
        available_room: dec!(5000),
    };
    let flows = [
        put(day(2028, 2, 29), dec!(1000)),
        put(day(2028, 3, 1), dec!(2000)),
    ];
    let room = rrsp_room(&start, &flows, &rules(), day(2028, 6, 1)).expect("computes");
    assert_eq!(room.window_start, day(2028, 3, 1));
    assert_eq!(room.before_window_cad, dec!(1000));
    assert_eq!(room.remaining_cad, dec!(3000));
    assert!(!room.outdated);
}

#[test]
fn fhsa_carries_forward_unused_room_up_to_8000() {
    let start = FhsaStart { opened_year: 2023 };
    let flows = [
        put(day(2023, 6, 1), dec!(3000)),
        put(day(2024, 4, 1), dec!(13000)),
        flow(day(2026, 6, 10), FlowKind::RrspTransfer, dec!(7000)),
        put(day(2026, 9, 1), dec!(10000)),
    ];
    let room = fhsa_room(&start, &flows, &rules(), day(2026, 11, 13)).expect("computes");

    let by_year: Vec<_> = room
        .years
        .iter()
        .map(|y| (y.year, y.room_cad, y.carryforward_cad))
        .collect();
    assert_eq!(
        by_year,
        vec![
            (2023, dec!(8000), dec!(0)),
            (2024, dec!(13000), dec!(5000)),
            (2025, dec!(8000), dec!(0)),
            (2026, dec!(16000), dec!(8000)),
        ]
    );
    assert_eq!(room.rrsp_transfers_cad, dec!(7000));
    assert_eq!(room.contributions_cad, dec!(10000));
    assert_eq!(room.remaining_cad, dec!(-1000));
    assert_eq!(room.excess_cad, dec!(1000));
    assert_eq!(room.estimated_tax_cad, dec!(30));
    assert_eq!(room.lifetime_used_cad, dec!(33000));
    assert_eq!(room.lifetime_remaining_cad, dec!(7000));
    // $8,000 less this year's excess, within the $7,000 lifetime left.
    assert_eq!(room.next_year_room_cad, dec!(7000));
}

#[test]
fn fhsa_excess_at_year_end_reduces_next_years_room() {
    // The CRA's example: a $5,000 excess at the end of 2025 leaves $3,000.
    let start = FhsaStart { opened_year: 2025 };
    let flows = [put(day(2025, 12, 1), dec!(13000))];
    let room = fhsa_room(&start, &flows, &rules(), day(2026, 1, 15)).expect("computes");
    assert_eq!(room.years[0].room_cad, dec!(8000));
    assert_eq!(room.room_cad, dec!(3000));
    assert_eq!(room.carryforward_cad, dec!(0));
    assert_eq!(room.excess_cad, dec!(0));
    assert_eq!(room.remaining_cad, dec!(3000));
}

#[test]
fn fhsa_lifetime_limit_caps_the_room() {
    let start = FhsaStart { opened_year: 2023 };
    let flows = [
        put(day(2023, 1, 10), dec!(8000)),
        put(day(2024, 1, 10), dec!(8000)),
        put(day(2025, 1, 10), dec!(8000)),
        put(day(2026, 1, 10), dec!(8000)),
    ];
    let room = fhsa_room(&start, &flows, &rules(), day(2026, 12, 31)).expect("computes");
    assert_eq!(room.lifetime_used_cad, dec!(32000));
    assert_eq!(room.next_year_room_cad, dec!(8000));
    assert_eq!(room.lifetime_remaining_cad, dec!(8000));
    assert!(!room.participation_period_may_have_ended);
}

#[test]
fn profiles_are_checked_when_read() {
    let profile = HolderProfile::from_json(
        r#"{"tfsa":{"basis":"craRoom","year":2026,"room":"12500"},"fhsa":{"openedYear":2024}}"#,
    )
    .expect("a valid profile");
    assert_eq!(
        profile.tfsa,
        Some(TfsaStart::CraRoom {
            year: 2026,
            room: dec!(12500)
        })
    );
    let round_trip =
        HolderProfile::from_json(&profile.to_json().expect("serializes")).expect("reads back");
    assert_eq!(round_trip, profile);

    for bad in [
        r#"{"fhsa":{"openedYear":2022}}"#,
        r#"{"tfsa":{"basis":"craRoom","year":2008,"room":"1"}}"#,
        r#"{"tfsa":{"basis":"guess"}}"#,
        r#"{"other":1}"#,
    ] {
        assert!(HolderProfile::from_json(bad).is_err(), "{bad}");
    }
}

#[test]
fn starts_in_the_future_are_refused() {
    let start = FhsaStart { opened_year: 2027 };
    assert!(fhsa_room(&start, &[], &rules(), day(2026, 5, 1)).is_err());
    let start = TfsaStart::CraRoom {
        year: 2027,
        room: dec!(1),
    };
    assert!(tfsa_room(&start, &[], &rules(), day(2026, 5, 1)).is_err());
}
