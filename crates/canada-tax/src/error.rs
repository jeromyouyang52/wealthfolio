use rust_decimal::Decimal;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("The rule table cannot be read: {0}")]
    InvalidRules(String),

    #[error("The account's Canadian settings cannot be read: {0}")]
    InvalidAccountSettings(String),

    #[error("{0} must be greater than zero")]
    NotPositive(&'static str),

    #[error("{0} cannot be negative")]
    Negative(&'static str),

    #[error("The position holds no units")]
    NoUnitsHeld,

    #[error("Cannot dispose of {requested} units: the pool holds {held}")]
    InsufficientUnits { requested: Decimal, held: Decimal },
}
