//! The LoonTax broker templates are in a migrated database, as system CSV
//! templates whose configs are JSON objects.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
mod common;

use diesel::prelude::*;
use wealthfolio_storage_sqlite::db;

#[test]
fn questrade_and_ibkr_templates_are_seeded() {
    let dir = common::TestDir::new();
    let access = db::DbAccess::plaintext(dir.path().join("app.db").to_str().unwrap());
    access.prepare().unwrap();
    access.run_migrations().unwrap();
    let mut conn = access.connect().unwrap();

    #[derive(QueryableByName)]
    struct Row {
        #[diesel(sql_type = diesel::sql_types::Text)]
        id: String,
        #[diesel(sql_type = diesel::sql_types::Text)]
        scope: String,
        #[diesel(sql_type = diesel::sql_types::Text)]
        kind: String,
        #[diesel(sql_type = diesel::sql_types::Text)]
        config: String,
    }
    let rows: Vec<Row> = diesel::sql_query(
        "SELECT id, scope, kind, config FROM import_templates \
         WHERE id IN ('system_questrade', 'system_ibkr_flex_trades') ORDER BY id",
    )
    .load(&mut conn)
    .unwrap();

    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.scope, "SYSTEM", "{}", row.id);
        assert_eq!(row.kind, "CSV_ACTIVITY", "{}", row.id);
        let config: serde_json::Value = serde_json::from_str(&row.config).unwrap();
        assert!(config["fieldMappings"]["date"].is_string(), "{}", row.id);
        assert!(config["activityMappings"]["BUY"].is_array(), "{}", row.id);
    }
}
