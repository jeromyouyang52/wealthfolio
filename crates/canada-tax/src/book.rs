//! From recorded activities to one ledger per holder and security.
//!
//! This is where accounts stop mattering: a holder's non-registered accounts
//! feed one pool for each security, and their registered plans only count
//! units for the superficial loss test.
//!
//! A security is computed only when everything about it is known. Otherwise
//! it gets an [`Issue`] and no ledger: a figure built on a guessed rate, an
//! unclassified account or half a transfer would look as good as a right one.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::acb::to_cad;
use crate::error::Error;
use crate::ledger::{Event, EventKind, Ledger};
use crate::registration::CanadaAccountInfo;
use crate::rules::SuperficialLossRules;

const CAD: &str = "CAD";

/// Canadian dollars for one unit of a currency on a date. `None` when the
/// rate is not known; it is never replaced by a nearby day's rate here.
pub trait FxRates {
    fn cad_per_unit(&self, currency: &str, date: NaiveDate) -> Option<Decimal>;
}

/// One recorded activity on a security, as the tax ledger needs it. Amounts
/// are in the activity's `currency` and are final: a purchase's cost includes
/// its charges, a sale's proceeds are net of them.
#[derive(Debug, Clone, PartialEq)]
pub struct TaxActivity {
    pub id: String,
    pub account_id: String,
    pub security_id: String,
    pub date: NaiveDate,
    pub currency: String,
    pub kind: TaxActivityKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaxActivityKind {
    Buy {
        units: Decimal,
        cost: Decimal,
    },
    Sell {
        units: Decimal,
        net_proceeds: Decimal,
    },
    Split {
        ratio: Decimal,
    },
    ReturnOfCapital {
        amount: Decimal,
    },
    NotionalDistribution {
        amount: Decimal,
    },
    /// Units moved between accounts. `paired_account_id` is the other
    /// account when both legs are recorded.
    TransferIn {
        units: Decimal,
        paired_account_id: Option<String>,
    },
    TransferOut {
        units: Decimal,
        paired_account_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub holder: String,
    pub security_id: String,
    pub ledger: Ledger,
}

/// Why a security has no ledger.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub security_id: String,
    /// `None` when the holder is what is unknown.
    pub holder: Option<String>,
    pub reason: IssueReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IssueReason {
    /// The account has no CRA registration set, so its holder and whether
    /// its trades are taxable are unknown.
    UnclassifiedAccount {
        account_id: String,
    },
    MissingFxRate {
        currency: String,
        date: NaiveDate,
    },
    /// A transfer that is not between two of the holder's accounts of the
    /// same kind: its cost, or the deemed sale it may be, has to be entered.
    UnsupportedTransfer {
        activity_id: String,
    },
    /// Two accounts record splits a day apart or less with different ratios.
    ConflictingSplits {
        date: NaiveDate,
    },
    /// The history does not add up, such as a sale of more units than held.
    InvalidHistory(Error),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaxBook {
    pub positions: Vec<Position>,
    pub issues: Vec<Issue>,
}

#[derive(Default)]
struct Draft {
    events: Vec<Event>,
    /// Splits already taken, to count one split once across accounts.
    splits: Vec<(NaiveDate, Decimal)>,
    issue: Option<IssueReason>,
}

impl TaxBook {
    /// Builds the ledgers. `activities` are in the order they happened;
    /// `accounts` gives each account's registration, `None` or absent when
    /// the user has not set it.
    pub fn build(
        activities: &[TaxActivity],
        accounts: &HashMap<String, Option<CanadaAccountInfo>>,
        fx: &dyn FxRates,
        rule: &SuperficialLossRules,
        as_of: NaiveDate,
    ) -> Self {
        let info = |account_id: &str| accounts.get(account_id).and_then(Option::as_ref);
        let mut book = Self::default();

        // An unclassified account could be anyone's and of any kind, so every
        // security it touches is left out for every holder.
        let mut unclassified: BTreeMap<&str, &str> = BTreeMap::new();
        for activity in activities {
            if info(&activity.account_id).is_none() {
                unclassified
                    .entry(&activity.security_id)
                    .or_insert(&activity.account_id);
            }
        }
        for (security_id, account_id) in &unclassified {
            book.issues.push(Issue {
                security_id: (*security_id).to_string(),
                holder: None,
                reason: IssueReason::UnclassifiedAccount {
                    account_id: (*account_id).to_string(),
                },
            });
        }

        let mut drafts: BTreeMap<(String, String), Draft> = BTreeMap::new();
        for activity in activities {
            if unclassified.contains_key(activity.security_id.as_str()) {
                continue;
            }
            let Some(account) = info(&activity.account_id) else {
                continue;
            };
            let draft = drafts
                .entry((account.holder.clone(), activity.security_id.clone()))
                .or_default();
            if draft.issue.is_some() {
                continue;
            }
            match event_for(activity, account, info, fx, &mut draft.splits) {
                Ok(Some(kind)) => draft.events.push(Event {
                    date: activity.date,
                    kind,
                }),
                Ok(None) => {}
                Err(reason) => draft.issue = Some(reason),
            }
        }

        for ((holder, security_id), draft) in drafts {
            let outcome = match draft.issue {
                Some(reason) => Err(reason),
                None => {
                    Ledger::compute(draft.events, rule, as_of).map_err(IssueReason::InvalidHistory)
                }
            };
            match outcome {
                Ok(ledger) => book.positions.push(Position {
                    holder,
                    security_id,
                    ledger,
                }),
                Err(reason) => book.issues.push(Issue {
                    security_id,
                    holder: Some(holder),
                    reason,
                }),
            }
        }
        book
    }

    /// Securities with at least one issue.
    pub fn securities_with_issues(&self) -> BTreeSet<&str> {
        self.issues
            .iter()
            .map(|issue| issue.security_id.as_str())
            .collect()
    }
}

/// The ledger event an activity is, `None` when it changes nothing for the
/// holder, or the reason the security cannot be computed.
fn event_for<'a>(
    activity: &TaxActivity,
    account: &CanadaAccountInfo,
    info: impl Fn(&str) -> Option<&'a CanadaAccountInfo>,
    fx: &dyn FxRates,
    splits: &mut Vec<(NaiveDate, Decimal)>,
) -> Result<Option<EventKind>, IssueReason> {
    let taxable = account.registration.counts_toward_acb();
    let cad = |amount: Decimal| -> Result<Decimal, IssueReason> {
        if activity.currency.eq_ignore_ascii_case(CAD) {
            return Ok(amount);
        }
        let missing = || IssueReason::MissingFxRate {
            currency: activity.currency.clone(),
            date: activity.date,
        };
        let rate = fx
            .cad_per_unit(&activity.currency, activity.date)
            .ok_or_else(missing)?;
        to_cad(amount, rate).map_err(|_| missing())
    };

    Ok(match &activity.kind {
        TaxActivityKind::Buy { units, cost } if taxable => Some(EventKind::Acquire {
            units: *units,
            cost_cad: cad(*cost)?,
        }),
        TaxActivityKind::Buy { units, .. } => Some(EventKind::RegisteredAcquire { units: *units }),
        TaxActivityKind::Sell {
            units,
            net_proceeds,
        } if taxable => Some(EventKind::Dispose {
            units: *units,
            net_proceeds_cad: cad(*net_proceeds)?,
        }),
        TaxActivityKind::Sell { units, .. } => Some(EventKind::RegisteredDispose { units: *units }),
        TaxActivityKind::Split { ratio } => {
            let near = |date: &NaiveDate| (*date - activity.date).num_days().abs() <= 1;
            match splits.iter().find(|(date, _)| near(date)) {
                Some((_, taken)) if taken == ratio => None,
                Some(_) => {
                    return Err(IssueReason::ConflictingSplits {
                        date: activity.date,
                    })
                }
                None => {
                    splits.push((activity.date, *ratio));
                    Some(EventKind::Split { ratio: *ratio })
                }
            }
        }
        TaxActivityKind::ReturnOfCapital { amount } if taxable => {
            Some(EventKind::ReturnOfCapital {
                amount_cad: cad(*amount)?,
            })
        }
        TaxActivityKind::NotionalDistribution { amount } if taxable => {
            Some(EventKind::NotionalDistribution {
                amount_cad: cad(*amount)?,
            })
        }
        // Inside a registered plan the cost is not tracked.
        TaxActivityKind::ReturnOfCapital { .. } | TaxActivityKind::NotionalDistribution { .. } => {
            None
        }
        TaxActivityKind::TransferIn {
            paired_account_id, ..
        }
        | TaxActivityKind::TransferOut {
            paired_account_id, ..
        } => {
            // Between two of the holder's accounts of the same kind nothing
            // changes: the pool, or the registered units, are the holder's.
            let same_side = paired_account_id
                .as_deref()
                .and_then(&info)
                .is_some_and(|other| {
                    other.holder == account.holder
                        && other.registration.counts_toward_acb() == taxable
                });
            if !same_side {
                return Err(IssueReason::UnsupportedTransfer {
                    activity_id: activity.id.clone(),
                });
            }
            None
        }
    })
}
