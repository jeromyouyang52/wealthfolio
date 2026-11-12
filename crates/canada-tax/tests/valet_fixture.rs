//! A real answer of the Bank of Canada Valet API, saved on 2026-11-12 from
//! `/valet/observations/FXUSDCAD/json?start_date=2026-06-29&end_date=2026-07-03`.

use chrono::NaiveDate;
use loontax_canada_tax::{parse_observations, FxRates, RateTable};
use rust_decimal_macros::dec;

const ANSWER: &str = include_str!("fixtures/valet-FXUSDCAD-2026-06-29-to-2026-07-03.json");

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).expect("the cases use real dates")
}

#[test]
fn reads_the_real_answer() {
    let rates = parse_observations(ANSWER, "USD").unwrap();
    assert_eq!(
        rates,
        vec![
            (day(6, 29), dec!(1.4206)),
            (day(6, 30), dec!(1.4210)),
            (day(7, 2), dec!(1.4181)),
            (day(7, 3), dec!(1.4201)),
        ]
    );
}

/// The Bank published nothing on Canada Day: June 30's rate stands in.
#[test]
fn canada_day_has_no_rate_and_takes_june_30s() {
    let mut table = RateTable::new();
    table.extend("USD", parse_observations(ANSWER, "USD").unwrap());

    let quote = table.quote("USD", day(7, 1)).unwrap();
    assert_eq!(quote.cad_per_unit, dec!(1.4210));
    assert_eq!(quote.rate_date, day(6, 30));
}
