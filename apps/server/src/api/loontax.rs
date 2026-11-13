//! LoonTax: the Canadian tax report and contribution room.
//!
//! Reading the report may ask the Bank of Canada for exchange rates not
//! stored yet (currency codes and dates only). A failed request is part of
//! the report, not an error of this endpoint.

use std::sync::Arc;

use axum::{
    routing::{get, put},
    Json, Router,
};
use loontax_canada_tax_service::{
    current_contribution_room, current_tax_report, updated_profiles, HolderProfile, RoomReport,
    TaxReport, HOLDER_PROFILES_SETTING_KEY,
};
use serde::Deserialize;
use wealthfolio_core::accounts::AccountServiceTrait;
use wealthfolio_core::settings::SettingsServiceTrait;

use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
};

async fn get_tax_report(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<TaxReport>> {
    let timezone = state.timezone()?;

    // The whole history is read, off the async workers.
    let reader = state.clone();
    let (accounts, activities) = tokio::task::spawn_blocking(move || {
        Ok::<_, wealthfolio_core::Error>((
            reader.account_service.get_all_accounts()?,
            reader.activity_service.get_activities()?,
        ))
    })
    .await
    .map_err(|_| ApiError::Internal("The tax report could not be computed".to_string()))??;

    // The tax crates' messages name what failed and carry no stored values.
    let report = current_tax_report(
        &accounts,
        &activities,
        &timezone,
        state.loontax_rates.as_ref(),
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(report))
}

async fn get_contribution_room(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<RoomReport>> {
    let timezone = state.timezone()?;

    let reader = state.clone();
    let (accounts, activities, profiles) = tokio::task::spawn_blocking(move || {
        Ok::<_, wealthfolio_core::Error>((
            reader.account_service.get_all_accounts()?,
            reader.activity_service.get_activities()?,
            reader
                .settings_service
                .get_setting_value(HOLDER_PROFILES_SETTING_KEY)?,
        ))
    })
    .await
    .map_err(|_| ApiError::Internal("The contribution room could not be computed".to_string()))??;

    let report = current_contribution_room(
        &accounts,
        &activities,
        profiles.as_deref(),
        &timezone,
        state.loontax_rates.as_ref(),
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(report))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveHolderProfile {
    holder: String,
    profile: HolderProfile,
}

async fn save_holder_profile(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<SaveHolderProfile>,
) -> ApiResult<Json<HolderProfile>> {
    let stored = state
        .settings_service
        .get_setting_value(HOLDER_PROFILES_SETTING_KEY)?;
    let updated = updated_profiles(stored.as_deref(), &body.holder, body.profile.clone())
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    state
        .settings_service
        .set_setting_value(HOLDER_PROFILES_SETTING_KEY, &updated)
        .await?;
    Ok(Json(body.profile))
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/loontax/tax-report", get(get_tax_report))
        .route("/loontax/contribution-room", get(get_contribution_room))
        .route("/loontax/holder-profiles", put(save_holder_profile))
}
