#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! `PUT /api/v1/loontax/holder-profiles` and
//! `GET /api/v1/loontax/contribution-room`: the figures a holder enters are
//! stored, and the room follows the deposits recorded in their plans. The
//! deposits are in Canadian dollars, so the Bank of Canada is not called.
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
async fn saved_figures_and_recorded_deposits_give_the_room() {
    let temp_dir = tempdir().unwrap();
    let path = |name: &str| temp_dir.path().join(name).to_string_lossy().into_owned();
    let config = test_config(path("app.db"), path("addons"));
    let state = build_state(&config).await.unwrap();
    let app = app_router(state.clone(), &config).unwrap();

    let tfsa = create_account(&app, "TFSA", Some("TFSA")).await;
    let fhsa = create_account(&app, "FHSA", Some("FHSA")).await;
    let year = chrono::Utc::now().format("%Y").to_string();

    // Before any figure is entered, both plans ask for one.
    let (status, report) = send(&app, "GET", "/api/v1/loontax/contribution-room", None).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["holders"][0]["holder"], "self");
    assert_eq!(report["holders"][0]["tfsa"]["status"], "needsStart");
    assert_eq!(report["holders"][0]["fhsa"]["status"], "needsStart");
    assert!(report["holders"][0]["rrsp"].is_null());

    let db = state.db_access.connect_rusqlite().unwrap();
    let deposit = |id: &str, account: &str, amount: &str| {
        let date = format!("{year}-01-05T15:00:00Z");
        db.execute(
            "INSERT INTO activities (id, account_id, activity_type, status, activity_date,
                                     amount, currency, created_at, updated_at)
             VALUES (?1, ?2, 'DEPOSIT', 'POSTED', ?3, ?4, 'CAD', ?3, ?3)",
            [id, account, &date, amount],
        )
        .unwrap();
    };
    deposit("1", &tfsa, "2500");
    deposit("2", &fhsa, "3000");

    let (status, saved) = send(
        &app,
        "PUT",
        "/api/v1/loontax/holder-profiles",
        Some(serde_json::json!({
            "holder": "self",
            "profile": {
                "tfsa": {"basis": "craRoom", "year": year.parse::<i32>().unwrap(), "room": "10000"},
                "fhsa": {"openedYear": year.parse::<i32>().unwrap()}
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");

    let (_, report) = send(&app, "GET", "/api/v1/loontax/contribution-room", None).await;
    let me = &report["holders"][0];
    assert_eq!(me["tfsa"]["status"], "ok");
    assert_eq!(me["tfsa"]["room"]["availableCad"], 7500.0);
    assert_eq!(me["fhsa"]["room"]["remainingCad"], 5000.0);
    assert_eq!(
        me["profile"]["fhsa"]["openedYear"],
        year.parse::<i32>().unwrap()
    );

    // A figure that cannot be right is refused and nothing changes.
    let (status, _) = send(
        &app,
        "PUT",
        "/api/v1/loontax/holder-profiles",
        Some(serde_json::json!({"holder": "self", "profile": {"fhsa": {"openedYear": 2019}}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, report) = send(&app, "GET", "/api/v1/loontax/contribution-room", None).await;
    assert_eq!(report["holders"][0]["fhsa"]["status"], "ok");

    // An empty profile clears the holder's figures.
    let (status, _) = send(
        &app,
        "PUT",
        "/api/v1/loontax/holder-profiles",
        Some(serde_json::json!({"holder": "self", "profile": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, report) = send(&app, "GET", "/api/v1/loontax/contribution-room", None).await;
    assert_eq!(report["holders"][0]["tfsa"]["status"], "needsStart");
}
