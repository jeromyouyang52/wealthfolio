//! How an account is registered with the CRA, stored in the account's `meta`.
//!
//! Upstream treats investment wrappers as outside its account types (a TFSA
//! is a `SECURITIES` account), so the registration lives under its own key in
//! `meta`, beside upstream's `accounting` and `allocation` entries:
//!
//! ```json
//! { "canada": { "registration": "TFSA", "holder": "self" } }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// The `meta` entry this crate owns.
pub const CANADA_META_KEY: &str = "canada";

/// The holder recorded when the user has not named one.
pub const DEFAULT_HOLDER: &str = "self";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Registration {
    NonRegistered,
    Tfsa,
    Rrsp,
    SpousalRrsp,
    Fhsa,
    Resp,
    Lira,
    Rrif,
}

impl Registration {
    pub fn is_registered(self) -> bool {
        self != Self::NonRegistered
    }

    /// Only non-registered holdings have an adjusted cost base: gains inside a
    /// registered plan are not capital gains of the holder.
    pub fn counts_toward_acb(self) -> bool {
        self == Self::NonRegistered
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanadaAccountInfo {
    pub registration: Registration,
    /// Whose account it is. Identical securities pool per holder.
    #[serde(default = "default_holder")]
    pub holder: String,
}

fn default_holder() -> String {
    DEFAULT_HOLDER.to_string()
}

impl CanadaAccountInfo {
    /// Reads the registration from an account's `meta`.
    ///
    /// `Ok(None)` means the user has not said how the account is registered.
    /// That is never read as non-registered: an unclassified TFSA would put
    /// its trades on a capital gains report. Settings that cannot be read are
    /// an error for the same reason.
    pub fn from_meta(meta: Option<&str>) -> Result<Option<Self>> {
        let Some(raw) = meta.map(str::trim).filter(|v| !v.is_empty()) else {
            return Ok(None);
        };
        // The reason names what failed and never echoes the stored value.
        let unreadable = |reason: &str| Error::InvalidAccountSettings(reason.to_string());

        let meta: Value =
            serde_json::from_str(raw).map_err(|_| unreadable("the account meta is not JSON"))?;
        let Some(entry) = meta.get(CANADA_META_KEY) else {
            return Ok(None);
        };
        let info: Self = serde_json::from_value(entry.clone())
            .map_err(|_| unreadable("the canada entry has an unknown shape or code"))?;
        if info.holder.trim().is_empty() {
            return Err(unreadable("the holder is empty"));
        }
        Ok(Some(info))
    }

    /// Writes the registration into `meta`, keeping every other entry.
    pub fn write_to_meta(&self, meta: Option<&str>) -> Result<String> {
        let mut root = match meta.map(str::trim).filter(|v| !v.is_empty()) {
            None => Value::Object(serde_json::Map::new()),
            Some(raw) => serde_json::from_str::<Value>(raw).map_err(|_| {
                Error::InvalidAccountSettings("the account meta is not JSON".to_string())
            })?,
        };
        let Some(entries) = root.as_object_mut() else {
            return Err(Error::InvalidAccountSettings(
                "the account meta is not a JSON object".to_string(),
            ));
        };
        let entry = serde_json::to_value(self).map_err(|_| {
            Error::InvalidAccountSettings("the canada entry cannot be written".to_string())
        })?;
        entries.insert(CANADA_META_KEY.to_string(), entry);
        Ok(root.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_meta_or_entry_is_unclassified_not_non_registered() {
        assert_eq!(CanadaAccountInfo::from_meta(None), Ok(None));
        assert_eq!(CanadaAccountInfo::from_meta(Some("  ")), Ok(None));
        assert_eq!(
            CanadaAccountInfo::from_meta(Some(r#"{"accounting":{"costBasisMethod":"WAC"}}"#)),
            Ok(None)
        );
    }

    #[test]
    fn reads_registration_and_defaults_the_holder() {
        let info = CanadaAccountInfo::from_meta(Some(r#"{"canada":{"registration":"TFSA"}}"#))
            .unwrap()
            .unwrap();
        assert_eq!(info.registration, Registration::Tfsa);
        assert_eq!(info.holder, DEFAULT_HOLDER);
        assert!(!info.registration.counts_toward_acb());

        let spousal = CanadaAccountInfo::from_meta(Some(
            r#"{"canada":{"registration":"SPOUSAL_RRSP","holder":"spouse"}}"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(spousal.registration, Registration::SpousalRrsp);
        assert_eq!(spousal.holder, "spouse");
    }

    #[test]
    fn unreadable_settings_are_errors_never_defaults() {
        for meta in [
            "{not json",
            r#"{"canada":"TFSA"}"#,
            r#"{"canada":{"registration":"ROTH_IRA"}}"#,
            r#"{"canada":{"registration":"TFSA","holder":" "}}"#,
            r#"{"canada":{"registration":"TFSA","extra":1}}"#,
        ] {
            assert!(
                matches!(
                    CanadaAccountInfo::from_meta(Some(meta)),
                    Err(Error::InvalidAccountSettings(_))
                ),
                "{meta}"
            );
        }
    }

    #[test]
    fn writing_keeps_the_other_meta_entries() {
        let info = CanadaAccountInfo {
            registration: Registration::NonRegistered,
            holder: DEFAULT_HOLDER.to_string(),
        };
        let written = info
            .write_to_meta(Some(
                r#"{"accounting":{"costBasisMethod":"WAC"},"broker_account_type":"MARGIN"}"#,
            ))
            .unwrap();
        let value: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["accounting"]["costBasisMethod"], "WAC");
        assert_eq!(value["broker_account_type"], "MARGIN");
        assert_eq!(value["canada"]["registration"], "NON_REGISTERED");
        assert_eq!(CanadaAccountInfo::from_meta(Some(&written)), Ok(Some(info)));
    }
}
