//! The contribution room the app shows: each holder's TFSA, RRSP and FHSA
//! room, from the figures they entered and the money recorded in their
//! registered accounts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use loontax_canada_tax::{
    fhsa_room, load_rates, rrsp_room, tfsa_room, CanadaAccountInfo, FhsaRoom, FlowKind, FxFallback,
    FxRates, HolderProfile, HolderProfiles, Plan, PlanFlow, RateSource, RateStore, Registration,
    Result, RrspRoom, Rules, TfsaRoom,
};
use rust_decimal::Decimal;
use serde::Serialize;
use wealthfolio_core::accounts::Account;
use wealthfolio_core::activities::{
    Activity, ActivityStatus, TransferPairResolution, ACTIVITY_TYPE_DEPOSIT,
    ACTIVITY_TYPE_TRANSFER_IN, ACTIVITY_TYPE_TRANSFER_OUT, ACTIVITY_TYPE_WITHDRAWAL,
};
use wealthfolio_core::utils::time_utils::activity_date_in_user_timezone;

use crate::report::RateFailure;

/// The app setting that holds every holder's profile, as JSON. Device sync
/// does not carry it: it is not one of the synced preference keys.
pub const HOLDER_PROFILES_SETTING_KEY: &str = "loontax_holder_profiles";

const CAD: &str = "CAD";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomReport {
    pub as_of: NaiveDate,
    pub holders: Vec<HolderRoom>,
    /// Recorded money left out of the figures, or assumed, and why.
    pub issues: Vec<RoomIssue>,
    pub fx_fallbacks: Vec<FxFallback>,
    pub rate_failures: Vec<RateFailure>,
    pub unreadable_account_ids: Vec<String>,
    /// The stored profiles could not be read; every plan needs its start again.
    pub unreadable_profiles: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderRoom {
    pub holder: String,
    pub profile: HolderProfile,
    pub tfsa: Option<PlanResult<TfsaRoom>>,
    pub rrsp: Option<PlanResult<RrspRoom>>,
    pub fhsa: Option<PlanResult<FhsaRoom>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanResult<T> {
    /// `ok`, `needsStart` (no figure entered) or `error`.
    pub status: &'static str,
    pub room: Option<T>,
    /// For `error`, what is wrong (a year missing from the rule table, a
    /// start in the future).
    pub detail: Option<String>,
    /// The holder has an account registered as this plan.
    pub has_account: bool,
    /// Some recorded money could not be counted (see the issues).
    pub incomplete: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomIssue {
    pub holder: String,
    pub plan: Plan,
    /// A stable code for the interface to translate.
    pub code: &'static str,
    pub activity_id: Option<String>,
    pub account_id: Option<String>,
    pub date: Option<NaiveDate>,
    pub currency: Option<String>,
    pub amount: Option<Decimal>,
}

impl RoomIssue {
    /// Whether the issue leaves recorded money out of the figures.
    fn excludes_money(&self) -> bool {
        !matches!(self.code, "ASSUMED_DIRECT_TRANSFER" | "FHSA_WITHDRAWAL")
    }
}

/// The report as of now, with the shipped rule table and the Bank of Canada.
pub async fn current_contribution_room(
    accounts: &[Account],
    activities: &[Activity],
    stored_profiles: Option<&str>,
    timezone: &str,
    store: &dyn RateStore,
) -> Result<RoomReport> {
    let rules = Rules::embedded()?;
    let bank = crate::bank_of_canada::BankOfCanada::new()?;
    let today = activity_date_in_user_timezone(chrono::Utc::now(), timezone);
    contribution_room(
        accounts,
        activities,
        stored_profiles,
        timezone,
        &rules,
        store,
        &bank,
        today,
    )
    .await
}

fn plan_of(registration: Registration) -> Option<Plan> {
    match registration {
        Registration::Tfsa => Some(Plan::Tfsa),
        Registration::Rrsp | Registration::SpousalRrsp => Some(Plan::Rrsp),
        Registration::Fhsa => Some(Plan::Fhsa),
        _ => None,
    }
}

/// A flow before conversion to Canadian dollars.
struct Recorded {
    holder: String,
    plan: Plan,
    activity_id: String,
    date: NaiveDate,
    currency: String,
    kind: FlowKind,
    amount: Decimal,
}

#[allow(clippy::too_many_arguments)]
pub async fn contribution_room(
    accounts: &[Account],
    activities: &[Activity],
    stored_profiles: Option<&str>,
    timezone: &str,
    rules: &Rules,
    store: &dyn RateStore,
    source: &dyn RateSource,
    today: NaiveDate,
) -> Result<RoomReport> {
    let mut registrations: HashMap<&str, CanadaAccountInfo> = HashMap::new();
    let mut unreadable_account_ids = Vec::new();
    for account in accounts {
        match CanadaAccountInfo::from_meta(account.meta.as_deref()) {
            Ok(Some(info)) => {
                registrations.insert(account.id.as_str(), info);
            }
            Ok(None) => {}
            Err(_) => unreadable_account_ids.push(account.id.clone()),
        }
    }

    let (profiles, unreadable_profiles) = match stored_profiles.map(str::trim) {
        None | Some("") => (HolderProfiles::new(), false),
        Some(raw) => match loontax_canada_tax::profiles_from_json(raw) {
            Ok(profiles) => (profiles, false),
            Err(_) => (HolderProfiles::new(), true),
        },
    };

    let mut issues = Vec::new();
    let recorded = recorded_flows(activities, &registrations, timezone, &mut issues);

    let needs: BTreeSet<(String, NaiveDate)> = recorded
        .iter()
        .filter(|flow| !flow.currency.eq_ignore_ascii_case(CAD))
        .map(|flow| (flow.currency.to_ascii_uppercase(), flow.date))
        .collect();
    let rates = load_rates(&needs, store, source, today).await?;

    let mut flows: BTreeMap<(String, Plan), Vec<PlanFlow>> = BTreeMap::new();
    let mut fx_fallbacks = Vec::new();
    for flow in recorded {
        let amount_cad = if flow.currency.eq_ignore_ascii_case(CAD) {
            flow.amount
        } else {
            match rates.table.quote(&flow.currency, flow.date) {
                Some(quote) => {
                    if quote.rate_date != flow.date {
                        fx_fallbacks.push(FxFallback {
                            activity_id: flow.activity_id.clone(),
                            currency: flow.currency.to_ascii_uppercase(),
                            date: flow.date,
                            rate_date: quote.rate_date,
                        });
                    }
                    (flow.amount * quote.cad_per_unit).round_dp(2)
                }
                None => {
                    issues.push(RoomIssue {
                        holder: flow.holder,
                        plan: flow.plan,
                        code: "MISSING_FX_RATE",
                        activity_id: Some(flow.activity_id),
                        account_id: None,
                        date: Some(flow.date),
                        currency: Some(flow.currency.to_ascii_uppercase()),
                        amount: Some(flow.amount),
                    });
                    continue;
                }
            }
        };
        flows
            .entry((flow.holder, flow.plan))
            .or_default()
            .push(PlanFlow {
                activity_id: flow.activity_id,
                date: flow.date,
                kind: flow.kind,
                amount_cad,
            });
    }

    // Holders with a registered plan account or a saved profile.
    let mut accounts_by_holder: BTreeMap<String, BTreeSet<Plan>> = BTreeMap::new();
    for info in registrations.values() {
        if let Some(plan) = plan_of(info.registration) {
            accounts_by_holder
                .entry(info.holder.clone())
                .or_default()
                .insert(plan);
        }
    }
    let holder_names: BTreeSet<String> = accounts_by_holder
        .keys()
        .chain(profiles.keys())
        .cloned()
        .collect();

    let mut holders = Vec::new();
    for holder in holder_names {
        let profile = profiles.get(&holder).cloned().unwrap_or_default();
        let plans = accounts_by_holder.get(&holder).cloned().unwrap_or_default();
        let empty = Vec::new();
        let flows_of = |plan: Plan| flows.get(&(holder.clone(), plan)).unwrap_or(&empty);
        let incomplete = |plan: Plan| {
            issues
                .iter()
                .any(|issue| issue.holder == holder && issue.plan == plan && issue.excludes_money())
        };
        let result =
            |plan: Plan, has_start: bool| (plans.contains(&plan) || has_start, incomplete(plan));

        let tfsa = {
            let (shown, incomplete) = result(Plan::Tfsa, profile.tfsa.is_some());
            shown.then(|| {
                plan_result(
                    profile
                        .tfsa
                        .as_ref()
                        .map(|start| tfsa_room(start, flows_of(Plan::Tfsa), rules, today)),
                    plans.contains(&Plan::Tfsa),
                    incomplete,
                )
            })
        };
        let rrsp = {
            let (shown, incomplete) = result(Plan::Rrsp, profile.rrsp.is_some());
            shown.then(|| {
                plan_result(
                    profile
                        .rrsp
                        .as_ref()
                        .map(|start| rrsp_room(start, flows_of(Plan::Rrsp), rules, today)),
                    plans.contains(&Plan::Rrsp),
                    incomplete,
                )
            })
        };
        let fhsa = {
            let (shown, incomplete) = result(Plan::Fhsa, profile.fhsa.is_some());
            shown.then(|| {
                plan_result(
                    profile
                        .fhsa
                        .as_ref()
                        .map(|start| fhsa_room(start, flows_of(Plan::Fhsa), rules, today)),
                    plans.contains(&Plan::Fhsa),
                    incomplete,
                )
            })
        };
        holders.push(HolderRoom {
            holder,
            profile,
            tfsa,
            rrsp,
            fhsa,
        });
    }

    Ok(RoomReport {
        as_of: today,
        holders,
        issues,
        fx_fallbacks,
        rate_failures: rates
            .failures
            .into_iter()
            .map(|failure| RateFailure {
                currency: failure.currency,
                from: failure.from,
                to: failure.to,
                detail: failure.error.to_string(),
            })
            .collect(),
        unreadable_account_ids,
        unreadable_profiles,
    })
}

fn plan_result<T>(
    computed: Option<Result<T>>,
    has_account: bool,
    incomplete: bool,
) -> PlanResult<T> {
    let (status, room, detail) = match computed {
        None => ("needsStart", None, None),
        Some(Ok(room)) => ("ok", Some(room), None),
        Some(Err(error)) => ("error", None, Some(error.to_string())),
    };
    PlanResult {
        status,
        room,
        detail,
        has_account,
        incomplete,
    }
}

/// The money recorded going into and out of TFSAs, RRSPs and FHSAs.
fn recorded_flows(
    activities: &[Activity],
    registrations: &HashMap<&str, CanadaAccountInfo>,
    timezone: &str,
    issues: &mut Vec<RoomIssue>,
) -> Vec<Recorded> {
    let pairs = TransferPairResolution::from_activities(activities);
    let mut ordered: Vec<&Activity> = activities.iter().collect();
    ordered.sort_by_key(|activity| activity.activity_date);

    let mut flows = Vec::new();
    for activity in ordered {
        if activity.status == ActivityStatus::Void {
            continue;
        }
        let Some(info) = registrations.get(activity.account_id.as_str()) else {
            continue;
        };
        let Some(plan) = plan_of(info.registration) else {
            continue;
        };
        let activity_type = activity.effective_type();
        let transfer_in = activity_type == ACTIVITY_TYPE_TRANSFER_IN;
        let transfer_out = activity_type == ACTIVITY_TYPE_TRANSFER_OUT;
        let date = activity_date_in_user_timezone(activity.activity_date, timezone);
        let issue = |code: &'static str| RoomIssue {
            holder: info.holder.clone(),
            plan,
            code,
            activity_id: Some(activity.id.clone()),
            account_id: Some(activity.account_id.clone()),
            date: Some(date),
            currency: Some(activity.currency.to_ascii_uppercase()),
            amount: amount_of(activity),
        };

        let kind = if activity_type == ACTIVITY_TYPE_DEPOSIT {
            FlowKind::Contribution
        } else if activity_type == ACTIVITY_TYPE_WITHDRAWAL {
            FlowKind::Withdrawal
        } else if transfer_in || transfer_out {
            let counterparty = pairs
                .pair_for_activity(&activity.id)
                .and_then(|pair| pair.counterparty_account_id(&activity.id))
                .map(str::to_string);
            let Some(counterparty) = counterparty else {
                // From or to another institution: usually a direct transfer
                // between the holder's own plans, which uses no room.
                issues.push(issue("ASSUMED_DIRECT_TRANSFER"));
                continue;
            };
            let Some(other) = registrations.get(counterparty.as_str()) else {
                issues.push(issue("TRANSFER_UNCLASSIFIED_ACCOUNT"));
                continue;
            };
            if other.holder != info.holder {
                issues.push(issue("TRANSFER_OTHER_HOLDER"));
                continue;
            }
            match (plan_of(other.registration), transfer_in) {
                // Between the holder's own plans of one kind.
                (Some(other_plan), _) if other_plan == plan => continue,
                (Some(Plan::Rrsp), true) if plan == Plan::Fhsa => FlowKind::RrspTransfer,
                (Some(Plan::Fhsa), true) if plan == Plan::Rrsp => continue,
                // FHSA to RRSP is a designated transfer; RRSP to FHSA is
                // counted on the FHSA side. Neither changes RRSP room.
                (Some(Plan::Rrsp | Plan::Fhsa), false) if plan != Plan::Tfsa => continue,
                (_, true) => FlowKind::Contribution,
                (_, false) => FlowKind::Withdrawal,
            }
        } else {
            continue;
        };

        if activity.status != ActivityStatus::Posted {
            issues.push(issue("UNCONFIRMED_ACTIVITY"));
            continue;
        }
        let Some(amount) = amount_of(activity) else {
            issues.push(issue("MISSING_AMOUNT"));
            continue;
        };
        if plan == Plan::Rrsp {
            match (info.registration, kind) {
                // RRSP withdrawals give no room back.
                (_, FlowKind::Withdrawal) => continue,
                // A spousal RRSP contribution uses the contributor's room,
                // and the app does not know who contributed.
                (Registration::SpousalRrsp, _) => {
                    issues.push(issue("SPOUSAL_RRSP_CONTRIBUTION"));
                    continue;
                }
                _ => {}
            }
        }
        if plan == Plan::Fhsa && kind == FlowKind::Withdrawal {
            issues.push(issue("FHSA_WITHDRAWAL"));
        }
        flows.push(Recorded {
            holder: info.holder.clone(),
            plan,
            activity_id: activity.id.clone(),
            date,
            currency: activity.currency.clone(),
            kind,
            amount,
        });
    }
    flows
}

/// The money an activity moved: its amount, or for property its units at
/// the recorded price.
fn amount_of(activity: &Activity) -> Option<Decimal> {
    activity
        .amount
        .or_else(|| Some(activity.quantity? * activity.unit_price?))
        .map(|amount| amount.abs())
        .filter(|amount| *amount > Decimal::ZERO)
}

/// The stored profiles with `holder`'s replaced by `profile`, ready to store
/// again. An empty profile removes the holder's entry. Stored profiles that
/// can no longer be read are replaced: the report already asks for them
/// again.
pub fn updated_profiles(
    stored: Option<&str>,
    holder: &str,
    profile: HolderProfile,
) -> Result<String> {
    loontax_canada_tax::check_holder(holder)?;
    profile.validate()?;
    let mut profiles = stored
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .and_then(|raw| loontax_canada_tax::profiles_from_json(raw).ok())
        .unwrap_or_default();
    if profile == HolderProfile::default() {
        profiles.remove(holder);
    } else {
        profiles.insert(holder.to_string(), profile);
    }
    loontax_canada_tax::profiles_to_json(&profiles)
}
