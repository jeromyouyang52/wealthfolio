//! The yearly rule table.
//!
//! Limits and rates change by year, so they are data (`rules/canada.json`),
//! each with the source it was checked against. A year the table does not
//! cover returns `None`: a limit the CRA has not announced is never guessed.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Deserialize;

use crate::error::{Error, Result};

const EMBEDDED_RULES: &str = include_str!("../rules/canada.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rules {
    pub tfsa: TfsaRules,
    pub rrsp: RrspRules,
    pub fhsa: FhsaRules,
    pub capital_gains: CapitalGainsRules,
    pub superficial_loss: SuperficialLossRules,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TfsaRules {
    pub source: String,
    pub annual_limits: BTreeMap<i32, Decimal>,
    pub over_contribution_monthly_rate: Decimal,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RrspRules {
    pub source: String,
    pub dollar_limits: BTreeMap<i32, Decimal>,
    pub earned_income_rate: Decimal,
    pub over_contribution_buffer: Decimal,
    pub over_contribution_monthly_rate: Decimal,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FhsaRules {
    pub source: String,
    pub first_year: i32,
    pub annual_limit: Decimal,
    pub lifetime_limit: Decimal,
    pub max_carry_forward: Decimal,
    pub over_contribution_monthly_rate: Decimal,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapitalGainsRules {
    pub source: String,
    pub inclusion_rates: Vec<InclusionRate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InclusionRate {
    pub from_year: i32,
    pub rate: Decimal,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SuperficialLossRules {
    pub source: String,
    pub days_before: u32,
    pub days_after: u32,
}

impl Rules {
    /// The table shipped with this build.
    pub fn embedded() -> Result<Self> {
        Self::from_json(EMBEDDED_RULES)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let rules: Self =
            serde_json::from_str(json).map_err(|e| Error::InvalidRules(e.to_string()))?;
        rules.validate()?;
        Ok(rules)
    }

    fn validate(&self) -> Result<()> {
        let invalid = |what: &str| Err(Error::InvalidRules(what.to_string()));
        let is_rate = |rate: Decimal| rate > Decimal::ZERO && rate <= Decimal::ONE;

        if self.tfsa.annual_limits.is_empty() || self.rrsp.dollar_limits.is_empty() {
            return invalid("a limit table is empty");
        }
        // Room accumulates year by year, so a gap would silently drop a year.
        let mut years = self.tfsa.annual_limits.keys();
        let mut previous = years.next().copied();
        for year in years {
            if previous.map(|p| p + 1) != Some(*year) {
                return invalid("the TFSA limit table skips a year");
            }
            previous = Some(*year);
        }
        let limits = self
            .tfsa
            .annual_limits
            .values()
            .chain(self.rrsp.dollar_limits.values())
            .chain([
                &self.fhsa.annual_limit,
                &self.fhsa.lifetime_limit,
                &self.fhsa.max_carry_forward,
            ]);
        if limits.into_iter().any(|limit| *limit <= Decimal::ZERO) {
            return invalid("a limit is not positive");
        }
        let rates = [
            self.tfsa.over_contribution_monthly_rate,
            self.rrsp.over_contribution_monthly_rate,
            self.rrsp.earned_income_rate,
            self.fhsa.over_contribution_monthly_rate,
        ];
        if !rates.into_iter().all(is_rate)
            || !self
                .capital_gains
                .inclusion_rates
                .iter()
                .all(|r| is_rate(r.rate))
        {
            return invalid("a rate is outside (0, 1]");
        }
        if self.capital_gains.inclusion_rates.is_empty() {
            return invalid("no capital gains inclusion rate");
        }
        if self.rrsp.over_contribution_buffer < Decimal::ZERO {
            return invalid("the RRSP over-contribution buffer is negative");
        }
        Ok(())
    }

    /// The TFSA dollar limit announced for `year`.
    pub fn tfsa_annual_limit(&self, year: i32) -> Option<Decimal> {
        self.tfsa.annual_limits.get(&year).copied()
    }

    /// Total TFSA room earned from `first_eligible_year` through `year` by
    /// someone who never contributed or withdrew. Eligibility starts the year
    /// the holder is 18 and resident, and no earlier than the table's first
    /// year. `None` when a year in the range has no announced limit.
    pub fn tfsa_cumulative_room(&self, first_eligible_year: i32, year: i32) -> Option<Decimal> {
        let first_table_year = *self.tfsa.annual_limits.keys().next()?;
        let start = first_eligible_year.max(first_table_year);
        if start > year {
            return Some(Decimal::ZERO);
        }
        (start..=year)
            .map(|y| self.tfsa_annual_limit(y))
            .sum::<Option<Decimal>>()
    }

    /// The RRSP dollar limit for `year` (the cap on new room; the holder's
    /// actual room is on their notice of assessment).
    pub fn rrsp_dollar_limit(&self, year: i32) -> Option<Decimal> {
        self.rrsp.dollar_limits.get(&year).copied()
    }

    /// The share of a capital gain realized in `year` that is taxable.
    pub fn capital_gains_inclusion_rate(&self, year: i32) -> Option<Decimal> {
        self.capital_gains
            .inclusion_rates
            .iter()
            .filter(|r| r.from_year <= year)
            .max_by_key(|r| r.from_year)
            .map(|r| r.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn embedded_table_loads() {
        Rules::embedded().unwrap();
    }

    #[test]
    fn tfsa_limits_match_the_published_figures() {
        let rules = Rules::embedded().unwrap();
        assert_eq!(rules.tfsa_annual_limit(2015), Some(dec!(10000)));
        assert_eq!(rules.tfsa_annual_limit(2026), Some(dec!(7000)));
        // Published total for someone eligible since 2009 who never contributed.
        assert_eq!(rules.tfsa_cumulative_room(2009, 2026), Some(dec!(109000)));
        // Eligibility before the TFSA existed earns nothing extra.
        assert_eq!(rules.tfsa_cumulative_room(1990, 2026), Some(dec!(109000)));
        // A newcomer in 2024: 7,000 x 3.
        assert_eq!(rules.tfsa_cumulative_room(2024, 2026), Some(dec!(21000)));
        assert_eq!(rules.tfsa_cumulative_room(2027, 2026), Some(dec!(0)));
    }

    #[test]
    fn an_unannounced_year_is_unknown_not_guessed() {
        let rules = Rules::embedded().unwrap();
        assert_eq!(rules.tfsa_annual_limit(2027), None);
        assert_eq!(rules.tfsa_cumulative_room(2009, 2027), None);
        assert_eq!(rules.rrsp_dollar_limit(2030), None);
    }

    #[test]
    fn rrsp_and_fhsa_figures() {
        let rules = Rules::embedded().unwrap();
        assert_eq!(rules.rrsp_dollar_limit(2026), Some(dec!(33810)));
        assert_eq!(rules.rrsp_dollar_limit(2027), Some(dec!(35390)));
        assert_eq!(rules.fhsa.annual_limit, dec!(8000));
        assert_eq!(rules.fhsa.lifetime_limit, dec!(40000));
    }

    #[test]
    fn inclusion_rate_is_one_half() {
        let rules = Rules::embedded().unwrap();
        assert_eq!(rules.capital_gains_inclusion_rate(2026), Some(dec!(0.5)));
        assert_eq!(rules.capital_gains_inclusion_rate(1999), None);
    }

    #[test]
    fn a_table_with_a_gap_or_a_bad_rate_is_refused() {
        let gap = EMBEDDED_RULES.replace(r#""2015": "10000","#, "");
        assert!(matches!(
            Rules::from_json(&gap),
            Err(Error::InvalidRules(_))
        ));
        let bad_rate = EMBEDDED_RULES.replace(r#""rate": "0.5""#, r#""rate": "1.5""#);
        assert!(matches!(
            Rules::from_json(&bad_rate),
            Err(Error::InvalidRules(_))
        ));
        assert!(matches!(
            Rules::from_json("{}"),
            Err(Error::InvalidRules(_))
        ));
    }
}
