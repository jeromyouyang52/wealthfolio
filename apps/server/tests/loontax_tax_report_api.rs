#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! `GET /api/v1/loontax/tax-report`: the Canadian tax report computed from
//! what the app stores. The trades here are in Canadian dollars, so no
//! exchange rate is asked for and the Bank of Canada is not called.
use std::{net::SocketAddr, time::Duration};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use tempfile::tempdir;
use tower::ServiceExt;
use wealthfolio_server::{api::app_router, build_state, config::Config};

fn test_config(db_path: String, addons_root: String) -> Config {
    Config {
        listen_addr: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        db_path,
        cors_allow: vec!["*".to_string()],
        request_timeout: Duration::from_secs(30),
        static_dir: "dist".to_string(),
        addons_root,
        raw_secret_key: vec![7; 32],
        secrets_encryption_key: [7; 32],
        database_key: [9; 32],
        db_encryption_required: false,
        auth: None,
        oidc: None,
        mcp_enabled: false,
        mcp_audit_enabled: true,
        mcp_allowed_hosts: None,
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, value)
}

/// Creates an account through the API, with its CRA registration in `meta`.
async fn create_account(app: &Router, name: &str, registration: Option<&str>) -> String {
    let meta =
        registration.map(|code| serde_json::json!({"canada": {"registration": code}}).to_string());
    let (status, account) = send(
        app,
        "POST",
        "/api/v1/accounts",
        Some(serde_json::json!({
            "name": name, "accountType": "SECURITIES", "currency": "CAD",
            "isDefault": false, "isActive": true, "trackingMode": "TRANSACTIONS",
            "meta": meta
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{account}");
    account["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn the_report_pools_across_accounts_and_applies_the_superficial_loss_rule() {
    let temp_dir = tempdir().unwrap();
    let path = |name: &str| temp_dir.path().join(name).to_string_lossy().into_owned();
    let config = test_config(path("app.db"), path("addons"));
    let state = build_state(&config).await.unwrap();
    let app = app_router(state.clone(), &config).unwrap();

    let broker_1 = create_account(&app, "Broker 1", Some("NON_REGISTERED")).await;
    let broker_2 = create_account(&app, "Broker 2", Some("NON_REGISTERED")).await;
    let tfsa = create_account(&app, "TFSA", Some("TFSA")).await;
    let unclassified = create_account(&app, "New", None).await;

    // Trades are written as rows: creating them through the API would look
    // the symbols up at a market data provider.
    let db = state.db_access.connect_rusqlite().unwrap();
    db.execute_batch(
        "INSERT INTO assets (id, kind, quote_mode, quote_ccy) VALUES
           ('XEQT', 'INVESTMENT', 'MANUAL', 'CAD'),
           ('VCN', 'INVESTMENT', 'MANUAL', 'CAD'),
           ('ZAG', 'INVESTMENT', 'MANUAL', 'CAD');",
    )
    .unwrap();
    let trade = |id: &str,
                 account: &str,
                 asset: &str,
                 kind: &str,
                 date: &str,
                 quantity: &str,
                 amount: &str| {
        db.execute(
            "INSERT INTO activities (id, account_id, asset_id, activity_type, status, activity_date,
                                     quantity, amount, currency, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'POSTED', ?5, ?6, ?7, 'CAD', ?5, ?5)",
            [id, account, asset, kind, date, quantity, amount],
        )
        .unwrap();
    };
    // XEQT: 2,000 + 4,000 over 200 shares = 30.00; 100 sold for 4,500 gain 1,500.
    trade(
        "1",
        &broker_1,
        "XEQT",
        "BUY",
        "2025-01-06T15:00:00Z",
        "100",
        "2000",
    );
    trade(
        "2",
        &broker_2,
        "XEQT",
        "BUY",
        "2025-02-03T15:00:00Z",
        "100",
        "4000",
    );
    trade(
        "3",
        &broker_2,
        "XEQT",
        "SELL",
        "2025-06-02T15:00:00Z",
        "100",
        "4500",
    );
    // VCN: 100 for 5,000, sold for 4,000, bought back in the TFSA eight days later.
    trade(
        "4",
        &broker_1,
        "VCN",
        "BUY",
        "2025-01-06T15:00:00Z",
        "100",
        "5000",
    );
    trade(
        "5",
        &broker_1,
        "VCN",
        "SELL",
        "2025-11-03T15:00:00Z",
        "100",
        "4000",
    );
    trade(
        "6",
        &tfsa,
        "VCN",
        "BUY",
        "2025-11-11T15:00:00Z",
        "100",
        "4100",
    );
    // ZAG sits in the account nobody classified.
    trade(
        "7",
        &unclassified,
        "ZAG",
        "BUY",
        "2025-01-06T15:00:00Z",
        "10",
        "150",
    );
    drop(db);

    let (status, report) = send(&app, "GET", "/api/v1/loontax/tax-report", None).await;
    assert_eq!(status, StatusCode::OK, "{report}");

    let holder = &report["holders"][0];
    assert_eq!(report["holders"].as_array().unwrap().len(), 1);
    assert_eq!(holder["holder"], "self");

    let year = &holder["years"][0];
    assert_eq!(year["year"], 2025);
    assert_eq!(year["gainsCad"], 1500.0);
    assert_eq!(year["lossesCad"], 0.0);
    assert_eq!(year["deniedLossesCad"], 1000.0);
    assert_eq!(year["taxableCad"], 750.0);

    let positions = holder["positions"].as_array().unwrap();
    let position = |id: &str| positions.iter().find(|p| p["securityId"] == id).unwrap();
    assert_eq!(position("XEQT")["acbTotalCad"], 3000.0);
    assert_eq!(position("XEQT")["acbPerUnitCad"], 30.0);
    assert_eq!(position("VCN")["dispositions"][0]["deniedLostCad"], 1000.0);

    let issues = report["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0]["securityId"], "ZAG");
    assert_eq!(issues[0]["code"], "UNCLASSIFIED_ACCOUNT");
    assert_eq!(issues[0]["accountId"], unclassified.as_str());

    assert_eq!(report["rateFailures"], serde_json::json!([]));
    assert_eq!(report["fxFallbacks"], serde_json::json!([]));
}
