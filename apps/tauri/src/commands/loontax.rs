//! LoonTax: the Canadian tax report.
//!
//! Reading the report may ask the Bank of Canada for exchange rates not
//! stored yet (currency codes and dates only). A failed request is part of
//! the report, not an error of this command.

use crate::profiles::ProfileAccess;

use log::debug;
use loontax_canada_tax_service::{current_tax_report, TaxReport};

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
