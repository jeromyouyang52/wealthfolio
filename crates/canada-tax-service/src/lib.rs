//! LoonTax services around the Canadian tax ledger (`loontax-canada-tax`),
//! which itself reads no database and calls no network.

pub mod activities;
pub mod bank_of_canada;
pub mod report;

pub use activities::tax_activities;
pub use bank_of_canada::BankOfCanada;
pub use report::{tax_report, TaxReport};
