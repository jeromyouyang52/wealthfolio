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

/// A rate and the day it was published for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxQuote {
    /// Canadian dollars for one unit of the currency.
    pub cad_per_unit: Decimal,
    pub rate_date: NaiveDate,
}

/// The rate to convert an amount of `date` with. A source may answer with an
/// earlier day's rate when none was published for `date` (see
/// [`crate::fx::RateTable`]); the book records each time that happens.
/// `None` when no rate is known: none is made up.
pub trait FxRates {
    fn quote(&self, currency: &str, date: NaiveDate) -> Option<FxQuote>;
}

/// An activity converted with another day's rate.
#[derive(Debug, Clone, PartialEq)]
pub struct FxFallback {
    pub activity_id: String,
    pub currency: String,
    pub date: NaiveDate,
    pub rate_date: NaiveDate,
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
    /// A record on the security that cannot be turned into one of the kinds
    /// above: a field is missing, or it is something not handled yet.
    Unreadable,
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
    /// A record that is incomplete or of a kind not handled yet.
    UnreadableActivity {
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
    /// Conversions made with an earlier day's rate, for the report to state.
    pub fx_fallbacks: Vec<FxFallback>,
}

#[derive(Default)]
struct Draft {
    events: Vec<Event>,
    /// Splits already taken, to count one split once across accounts.
    splits: Vec<(NaiveDate, Decimal)>,
    issue: Option<IssueReason>,
    fx_fallbacks: Vec<FxFallback>,
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
            let mut fallback = None;
            let event = event_for(
                activity,
                account,
                info,
                fx,
                &mut draft.splits,
                &mut fallback,
            );
            draft.fx_fallbacks.extend(fallback);
            match event {
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
                Ok(ledger) => {
                    book.fx_fallbacks.extend(draft.fx_fallbacks);
                    book.positions.push(Position {
                        holder,
                        security_id,
                        ledger,
                    });
                }
                Err(reason) => book.issues.push(Issue {
                    security_id,
                    holder: Some(holder),
                    reason,
                }),
            }
        }
        book
    }

    /// The (currency, date) pairs `build` will ask the rates for: the money
    /// activities in foreign currencies of classified non-registered accounts.
    pub fn fx_needs(
        activities: &[TaxActivity],
        accounts: &HashMap<String, Option<CanadaAccountInfo>>,
    ) -> BTreeSet<(String, NaiveDate)> {
        activities
            .iter()
            .filter(|activity| !activity.currency.eq_ignore_ascii_case(CAD))
            .filter(|activity| {
                matches!(
                    activity.kind,
                    TaxActivityKind::Buy { .. }
                        | TaxActivityKind::Sell { .. }
                        | TaxActivityKind::ReturnOfCapital { .. }
                        | TaxActivityKind::NotionalDistribution { .. }
                )
            })
            .filter(|activity| {
                accounts
                    .get(&activity.account_id)
                    .and_then(Option::as_ref)
                    .is_some_and(|account| account.registration.counts_toward_acb())
            })
            .map(|activity| (activity.currency.to_ascii_uppercase(), activity.date))
            .collect()
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
    fallback: &mut Option<FxFallback>,
) -> Result<Option<EventKind>, IssueReason> {
    let taxable = account.registration.counts_toward_acb();
    let mut cad = |amount: Decimal| -> Result<Decimal, IssueReason> {
        if activity.currency.eq_ignore_ascii_case(CAD) {
            return Ok(amount);
        }
        let missing = || IssueReason::MissingFxRate {
            currency: activity.currency.clone(),
            date: activity.date,
        };
        let quote = fx
            .quote(&activity.currency, activity.date)
            .ok_or_else(missing)?;
        if quote.rate_date != activity.date {
            *fallback = Some(FxFallback {
                activity_id: activity.id.clone(),
                currency: activity.currency.clone(),
                date: activity.date,
                rate_date: quote.rate_date,
            });
        }
        to_cad(amount, quote.cad_per_unit).map_err(|_| missing())
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
        TaxActivityKind::Unreadable => {
            return Err(IssueReason::UnreadableActivity {
                activity_id: activity.id.clone(),
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
