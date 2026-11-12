//! Adjusted cost base of one security for one holder, in Canadian dollars.
//!
//! Identical properties are averaged: every unit a holder owns outside
//! registered plans shares one cost, whichever account holds it. A pool
//! therefore belongs to a (holder, security) pair, and the caller feeds it the
//! trades of all that holder's non-registered accounts in date order.
//!
//! The pool is the arithmetic of one position. The dated history, and the
//! superficial loss rule that needs to look ahead in it, are in `ledger`.

use rust_decimal::Decimal;

use crate::error::{Error, Result};

/// Converts an amount to Canadian dollars at the rate of its own date
/// (CAD per one unit of the foreign currency). Each trade converts at its own
/// rate, so a currency move between purchase and sale is part of the gain.
pub fn to_cad(amount: Decimal, cad_per_unit: Decimal) -> Result<Decimal> {
    if cad_per_unit <= Decimal::ZERO {
        return Err(Error::NotPositive("The exchange rate"));
    }
    Ok(amount * cad_per_unit)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AcbPool {
    units: Decimal,
    total_cad: Decimal,
}

/// What one disposition relieved and realized, in Canadian dollars.
#[derive(Debug, Clone, PartialEq)]
pub struct Disposition {
    pub units: Decimal,
    /// Proceeds less the outlays and expenses of the sale.
    pub net_proceeds_cad: Decimal,
    pub acb_relieved_cad: Decimal,
    /// Negative for a capital loss.
    pub gain_cad: Decimal,
}

impl AcbPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn units(&self) -> Decimal {
        self.units
    }

    pub fn total_cad(&self) -> Decimal {
        self.total_cad
    }

    /// Average cost of one unit; `None` for an empty pool.
    pub fn per_unit_cad(&self) -> Option<Decimal> {
        (self.units > Decimal::ZERO).then(|| self.total_cad / self.units)
    }

    /// Adds units at their cost: the price paid plus commissions and fees,
    /// already in Canadian dollars.
    pub fn acquire(&mut self, units: Decimal, cost_cad: Decimal) -> Result<()> {
        if units <= Decimal::ZERO {
            return Err(Error::NotPositive("Units acquired"));
        }
        if cost_cad < Decimal::ZERO {
            return Err(Error::Negative("The cost of an acquisition"));
        }
        self.units += units;
        self.total_cad += cost_cad;
        Ok(())
    }

    /// Multiplies the units by a split ratio (2 for a 2-for-1 split, 0.1 for a
    /// 1-for-10 consolidation). The total cost does not change.
    pub fn split(&mut self, ratio: Decimal) -> Result<()> {
        if ratio <= Decimal::ZERO {
            return Err(Error::NotPositive("A split ratio"));
        }
        self.units *= ratio;
        Ok(())
    }

    /// Adds to the cost without adding units: a reinvested (notional)
    /// distribution, or a superficial loss carried into the shares kept.
    pub fn increase_cost(&mut self, amount_cad: Decimal) -> Result<()> {
        if amount_cad < Decimal::ZERO {
            return Err(Error::Negative("An increase of cost"));
        }
        if self.units <= Decimal::ZERO {
            return Err(Error::NoUnitsHeld);
        }
        self.total_cad += amount_cad;
        Ok(())
    }

    /// Reduces the cost by a return of capital. The cost cannot go below
    /// zero: the amount beyond it is returned, and is a capital gain of the
    /// year it was received.
    pub fn return_of_capital(&mut self, amount_cad: Decimal) -> Result<Decimal> {
        if amount_cad < Decimal::ZERO {
            return Err(Error::Negative("A return of capital"));
        }
        if self.units <= Decimal::ZERO {
            return Err(Error::NoUnitsHeld);
        }
        let excess = (amount_cad - self.total_cad).max(Decimal::ZERO);
        self.total_cad = (self.total_cad - amount_cad).max(Decimal::ZERO);
        Ok(excess)
    }

    /// Removes units at the pool's average cost. `net_proceeds_cad` is the sale
    /// price less commissions and fees, already in Canadian dollars; it may be
    /// negative when the charges exceed the price.
    pub fn dispose(&mut self, units: Decimal, net_proceeds_cad: Decimal) -> Result<Disposition> {
        if units <= Decimal::ZERO {
            return Err(Error::NotPositive("Units disposed"));
        }
        if units > self.units {
            return Err(Error::InsufficientUnits {
                requested: units,
                held: self.units,
            });
        }
        // Selling everything relieves the whole cost exactly, so no rounding
        // remainder of the division is left in an empty pool.
        let acb_relieved_cad = if units == self.units {
            self.total_cad
        } else {
            self.total_cad * units / self.units
        };
        self.units -= units;
        self.total_cad -= acb_relieved_cad;
        Ok(Disposition {
            units,
            net_proceeds_cad,
            acb_relieved_cad,
            gain_cad: net_proceeds_cad - acb_relieved_cad,
        })
    }
}
