//! Reading the app's recorded activities as the tax ledger's input.
//!
//! Only records on a security matter here. Each becomes one [`TaxActivity`],
//! or nothing when it cannot change a cost base (a cash dividend, a fee).
//! A record that should matter but cannot be read (a draft trade, a missing
//! quantity, a kind not handled yet) becomes `Unreadable`, which holds its
//! security back: leaving it out would give a confident wrong figure.

use loontax_canada_tax::{TaxActivity, TaxActivityKind};
use wealthfolio_core::activities::{
    Activity, ActivityStatus, TransferPairResolution, ACTIVITY_SUBTYPE_DIVIDEND_IN_KIND,
    ACTIVITY_SUBTYPE_DRIP, ACTIVITY_SUBTYPE_NOTIONAL_DISTRIBUTION,
    ACTIVITY_SUBTYPE_RETURN_OF_CAPITAL, ACTIVITY_SUBTYPE_STAKING_REWARD, ACTIVITY_TYPE_ADJUSTMENT,
    ACTIVITY_TYPE_BUY, ACTIVITY_TYPE_DIVIDEND, ACTIVITY_TYPE_INTEREST, ACTIVITY_TYPE_SELL,
    ACTIVITY_TYPE_SPLIT, ACTIVITY_TYPE_TRANSFER_IN, ACTIVITY_TYPE_TRANSFER_OUT,
    ACTIVITY_TYPE_UNKNOWN,
};
use wealthfolio_core::utils::time_utils::activity_date_in_user_timezone;

/// The tax ledger's view of `activities`, oldest first. `timezone` is the
/// user's: a trade belongs to the day it happened where they live.
pub fn tax_activities(activities: &[Activity], timezone: &str) -> Vec<TaxActivity> {
    let pairs = TransferPairResolution::from_activities(activities);
    let mut ordered: Vec<&Activity> = activities.iter().collect();
    ordered.sort_by_key(|activity| activity.activity_date);

    ordered
        .into_iter()
        .filter(|activity| activity.status != ActivityStatus::Void)
        .filter_map(|activity| {
            let security_id = activity.asset_id.clone()?;
            let kind = kind_of(activity, &pairs)?;
            Some(TaxActivity {
                id: activity.id.clone(),
                account_id: activity.account_id.clone(),
                security_id,
                date: activity_date_in_user_timezone(activity.activity_date, timezone),
                currency: activity.currency.clone(),
                kind,
            })
        })
        .collect()
}

/// `None` for a record that cannot change a cost base.
fn kind_of(activity: &Activity, pairs: &TransferPairResolution) -> Option<TaxActivityKind> {
    let subtype = activity.subtype.as_deref().map(str::trim).unwrap_or("");
    let acquired = || match (activity.quantity, activity.amount) {
        (Some(units), Some(cost)) => TaxActivityKind::Buy { units, cost },
        _ => TaxActivityKind::Unreadable,
    };
    let amount = |kind: fn(rust_decimal::Decimal) -> TaxActivityKind| {
        activity.amount.map_or(TaxActivityKind::Unreadable, kind)
    };
    let paired_account_id = || {
        pairs
            .pair_for_activity(&activity.id)
            .and_then(|pair| pair.counterparty_account_id(&activity.id))
            .map(str::to_string)
    };

    let kind = match (activity.effective_type(), subtype) {
        // Plain income changes no cost; its reinvested and capital forms do.
        (ACTIVITY_TYPE_DIVIDEND, ACTIVITY_SUBTYPE_RETURN_OF_CAPITAL) => {
            amount(|amount| TaxActivityKind::ReturnOfCapital { amount })
        }
        (ACTIVITY_TYPE_DIVIDEND, ACTIVITY_SUBTYPE_NOTIONAL_DISTRIBUTION) => {
            amount(|amount| TaxActivityKind::NotionalDistribution { amount })
        }
        (ACTIVITY_TYPE_DIVIDEND, ACTIVITY_SUBTYPE_DRIP | ACTIVITY_SUBTYPE_DIVIDEND_IN_KIND)
        | (ACTIVITY_TYPE_INTEREST, ACTIVITY_SUBTYPE_STAKING_REWARD) => acquired(),
        (ACTIVITY_TYPE_DIVIDEND | ACTIVITY_TYPE_INTEREST, _) => return None,

        (ACTIVITY_TYPE_BUY, "") => acquired(),
        (ACTIVITY_TYPE_SELL, "") => match (activity.quantity, activity.amount) {
            (Some(units), Some(net_proceeds)) => TaxActivityKind::Sell {
                units,
                net_proceeds,
            },
            _ => TaxActivityKind::Unreadable,
        },
        (ACTIVITY_TYPE_SPLIT, _) => amount(|ratio| TaxActivityKind::Split { ratio }),
        (ACTIVITY_TYPE_TRANSFER_IN, "") => match activity.quantity {
            Some(units) => TaxActivityKind::TransferIn {
                units,
                paired_account_id: paired_account_id(),
            },
            None => TaxActivityKind::Unreadable,
        },
        (ACTIVITY_TYPE_TRANSFER_OUT, "") => match activity.quantity {
            Some(units) => TaxActivityKind::TransferOut {
                units,
                paired_account_id: paired_account_id(),
            },
            None => TaxActivityKind::Unreadable,
        },
        // Trades and transfers with a subtype (short positions, option
        // expiries, conversions), adjustments and unclassified records.
        (
            ACTIVITY_TYPE_BUY
            | ACTIVITY_TYPE_SELL
            | ACTIVITY_TYPE_TRANSFER_IN
            | ACTIVITY_TYPE_TRANSFER_OUT
            | ACTIVITY_TYPE_ADJUSTMENT
            | ACTIVITY_TYPE_UNKNOWN,
            _,
        ) => TaxActivityKind::Unreadable,
        // Deposits, withdrawals, fees, taxes and credits that name a security.
        _ => return None,
    };

    // A pending or draft record is a real event not yet confirmed.
    Some(if activity.status == ActivityStatus::Posted {
        kind
    } else {
        TaxActivityKind::Unreadable
    })
}
