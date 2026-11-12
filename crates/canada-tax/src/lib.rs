//! Canadian tax ledger for LoonTax.
//!
//! This crate is pure calculation: it reads no database and calls no network.
//! Callers hand it activities already selected and converted, and it returns
//! tax figures. The portfolio engine keeps computing each account's book cost
//! (its rules R7.3 exclude pooling across accounts and look-ahead rules); the
//! figures here are the taxpayer's, across accounts, and are shown separately.
//!
//! Nothing here is tax advice. Every number in the rule table carries the
//! source it was checked against.

pub mod acb;
pub mod book;
pub mod error;
pub mod fx;
pub mod fx_sync;
pub mod ledger;
pub mod registration;
pub mod rules;

pub use acb::{to_cad, AcbPool, Disposition};
pub use book::{
    FxFallback, FxQuote, FxRates, Issue, IssueReason, Position, TaxActivity, TaxActivityKind,
    TaxBook,
};
pub use error::{Error, Result};
pub use fx::{parse_observations, series_name, RateTable, MAX_RATE_AGE_DAYS};
pub use fx_sync::{load_rates, FetchFailure, LoadedRates, RateSource, RateStore};
pub use ledger::{
    summarize_year, DeemedGain, DispositionRecord, Event, EventKind, Ledger, YearSummary,
};
pub use registration::{CanadaAccountInfo, Registration, CANADA_META_KEY, DEFAULT_HOLDER};
pub use rules::Rules;
