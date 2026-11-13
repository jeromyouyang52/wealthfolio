//! LoonTax: the Canadian tax report.
//!
//! Reading the report may ask the Bank of Canada for exchange rates not
//! stored yet (currency codes and dates only). A failed request is part of
//! the report, not an error of this endpoint.

use std::sync::Arc;

use axum::{routing::get, Json, Router};
use chrono::Utc;
use loontax_canada_tax::Rules;
use loontax_canada_tax_service::{tax_report, BankOfCanada, TaxReport};
use wealthfolio_core::{
    accounts::AccountServiceTrait, utils::time_utils::activity_date_in_user_timezone,
};

use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
};

// The tax crates' messages name what failed and carry no stored values.
fn internal(error: loontax_canada_tax::Error) -> ApiError {
    ApiError::Internal(error.to_string())
}

async fn get_tax_report(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<TaxReport>> {
    let timezone = state.timezone()?;
    let today = activity_date_in_user_timezone(Utc::now(), &timezone);

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

    let rules = Rules::embedded().map_err(internal)?;
    let bank = BankOfCanada::new().map_err(internal)?;
    let report = tax_report(
        &accounts,
        &activities,
        &timezone,
        &rules,
        state.loontax_rates.as_ref(),
        &bank,
        today,
    )
    .await
    .map_err(internal)?;
    Ok(Json(report))
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/loontax/tax-report", get(get_tax_report))
}
