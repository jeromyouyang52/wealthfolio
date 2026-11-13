//! LoonTax: the Canadian tax report and contribution room.
//!
//! Reading the report may ask the Bank of Canada for exchange rates not
//! stored yet (currency codes and dates only). A failed request is part of
//! the report, not an error of this command.

use crate::profiles::ProfileAccess;

use log::debug;
use loontax_canada_tax_service::{
    current_contribution_room, current_tax_report, updated_profiles, HolderProfile, RoomReport,
    TaxReport, HOLDER_PROFILES_SETTING_KEY,
};

#[tauri::command]
pub async fn get_tax_report(state: ProfileAccess) -> Result<TaxReport, String> {
    let context = state.context()?;
    debug!("Computing the Canadian tax report...");
    let timezone = context.get_timezone();

    // The whole history is read, off the async workers.
    let (accounts, activities) = {
        let account_service = context.account_service();
        let activity_service = context.activity_service();
        tauri::async_runtime::spawn_blocking(move || {
            Ok::<_, wealthfolio_core::Error>((
                account_service.get_all_accounts()?,
                activity_service.get_activities()?,
            ))
        })
        .await
        .map_err(|_| "The tax report could not be computed".to_string())?
        .map_err(|e| format!("Failed to read the records for the tax report: {}", e))?
    };

    // The tax crates' messages name what failed and carry no stored values.
    current_tax_report(
        &accounts,
        &activities,
        &timezone,
        context.loontax_rates().as_ref(),
    )
    .await
    .map_err(|e| format!("Failed to compute the tax report: {}", e))
}

#[tauri::command]
pub async fn get_contribution_room(state: ProfileAccess) -> Result<RoomReport, String> {
    let context = state.context()?;
    debug!("Computing the contribution room...");
    let timezone = context.get_timezone();

    let (accounts, activities, profiles) = {
        let account_service = context.account_service();
        let activity_service = context.activity_service();
        let settings_service = context.settings_service();
        tauri::async_runtime::spawn_blocking(move || {
            Ok::<_, wealthfolio_core::Error>((
                account_service.get_all_accounts()?,
                activity_service.get_activities()?,
                settings_service.get_setting_value(HOLDER_PROFILES_SETTING_KEY)?,
            ))
        })
        .await
        .map_err(|_| "The contribution room could not be computed".to_string())?
        .map_err(|e| {
            format!(
                "Failed to read the records for the contribution room: {}",
                e
            )
        })?
    };

    current_contribution_room(
        &accounts,
        &activities,
        profiles.as_deref(),
        &timezone,
        context.loontax_rates().as_ref(),
    )
    .await
    .map_err(|e| format!("Failed to compute the contribution room: {}", e))
}

#[tauri::command]
pub async fn save_holder_profile(
    holder: String,
    profile: HolderProfile,
    state: ProfileAccess,
) -> Result<HolderProfile, String> {
    let context = state.context()?;
    let settings = context.settings_service();
    let stored = settings
        .get_setting_value(HOLDER_PROFILES_SETTING_KEY)
        .map_err(|e| format!("Failed to read the contribution room settings: {}", e))?;
    let updated =
        updated_profiles(stored.as_deref(), &holder, profile.clone()).map_err(|e| e.to_string())?;
    settings
        .set_setting_value(HOLDER_PROFILES_SETTING_KEY, &updated)
        .await
        .map_err(|e| format!("Failed to save the contribution room settings: {}", e))?;
    Ok(profile)
}
