//! LoonTax keeps an account's CRA registration under a `canada` entry in the
//! account's `meta` (crate `loontax-canada-tax`). Upstream refuses accounting
//! settings it cannot read, so these tests pin that the extra entry leaves
//! them readable.

use wealthfolio_core::accounts::{Account, CostBasisMethod};

fn account_with_meta(meta: &str) -> Account {
    Account {
        id: "acc-1".to_string(),
        meta: Some(meta.to_string()),
        ..Account::default()
    }
}

#[test]
fn canada_entry_alone_leaves_the_default_accounting_settings() {
    let account = account_with_meta(r#"{"canada":{"registration":"TFSA","holder":"self"}}"#);
    let settings = account.accounting_settings().unwrap();
    assert_eq!(settings.cost_basis_method, CostBasisMethod::Fifo);
}

#[test]
fn canada_entry_beside_accounting_keeps_the_chosen_method() {
    let account = account_with_meta(
        r#"{"accounting":{"costBasisMethod":"WAC"},"canada":{"registration":"NON_REGISTERED","holder":"self"}}"#,
    );
    let settings = account.accounting_settings().unwrap();
    assert_eq!(settings.cost_basis_method, CostBasisMethod::Wac);
    settings.ensure_supported_for_calculation().unwrap();
}
