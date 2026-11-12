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
pub mod error;
pub mod registration;
pub mod rules;

pub use acb::{to_cad, AcbPool, Disposition};
pub use error::{Error, Result};
pub use registration::{CanadaAccountInfo, Registration, CANADA_META_KEY, DEFAULT_HOLDER};
pub use rules::Rules;
