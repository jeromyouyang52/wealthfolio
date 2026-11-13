//! LoonTax services around the Canadian tax ledger (`loontax-canada-tax`),
//! which itself reads no database and calls no network.

pub mod activities;
pub mod bank_of_canada;
pub mod report;
pub mod room;

pub use activities::tax_activities;
pub use bank_of_canada::BankOfCanada;
pub use loontax_canada_tax::HolderProfile;
pub use report::{current_tax_report, tax_report, TaxReport};
pub use room::{
    contribution_room, current_contribution_room, updated_profiles, RoomReport,
    HOLDER_PROFILES_SETTING_KEY,
};
