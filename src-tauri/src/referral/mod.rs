//! Affiliate links for exchange and swap partner programs.
//!
//! What this is *not*: a user→user referral system. Attributing "A invited B" needs a server —
//! the desktop app has nowhere tamper-proof to record it, and a localStorage-style marker is
//! editable in seconds. So the widget does the part that works honestly without a backend: it
//! holds the user's own affiliate IDs, builds the partner links, and reads back whatever
//! statistics the partner's API is willing to report. The attribution itself is the partner's
//! job, which is the only place it can be trusted.

pub mod commands;
pub mod links;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The persisted, non-secret half of the referral feature.
///
/// An affiliate ID is public by design — it travels in the URL that gets shared — so it belongs
/// in `settings.json`. A partner *API key* (used to read statistics) does not, and goes to the
/// credential store instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferralSettings {
    /// Partner shown in the panel. `None` until the user picks one.
    pub partner: Option<String>,
    /// Affiliate ID per partner, keyed by partner id. Kept per-partner rather than as a single
    /// field so switching partners in the dropdown doesn't discard the other IDs.
    pub ids: BTreeMap<String, String>,
    /// Link-format overrides, keyed by partner id. Empty for partners whose built-in default
    /// works; set when the dashboard hands out a different scheme — see `links.rs` for why
    /// that is treated as the normal case rather than an edge case.
    #[serde(default)]
    pub templates: BTreeMap<String, String>,
}

impl ReferralSettings {
    /// The affiliate ID for the currently selected partner, if both are set.
    pub fn active_id(&self) -> Option<&str> {
        let partner = self.partner.as_deref()?;
        self.ids.get(partner).map(String::as_str).filter(|id| !id.is_empty())
    }

    /// The link-format override for the currently selected partner, if any.
    pub fn active_template(&self) -> Option<&str> {
        let partner = self.partner.as_deref()?;
        self.templates
            .get(partner)
            .map(String::as_str)
            .filter(|t| !t.trim().is_empty())
    }

    /// The referral link as it stands: `None` when there is nothing to build one from,
    /// `Some(Err)` when the inputs are there but wrong (bad ID, missing template).
    pub fn active_link(&self) -> Option<Result<String, String>> {
        let partner = self.partner.as_deref()?;
        let id = self.active_id()?;
        Some(links::build(partner, id, self.active_template()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_no_partner_selected() {
        let settings = ReferralSettings::default();
        assert!(settings.partner.is_none());
        assert!(settings.active_id().is_none());
    }

    #[test]
    fn switching_partners_keeps_both_ids() {
        let mut settings = ReferralSettings {
            partner: Some("changenow".into()),
            ..Default::default()
        };
        settings.ids.insert("changenow".into(), "abc123".into());
        settings.ids.insert("binance".into(), "XY7788".into());

        assert_eq!(settings.active_id(), Some("abc123"));

        settings.partner = Some("binance".into());
        assert_eq!(settings.active_id(), Some("XY7788"));
    }

    #[test]
    fn builds_the_active_link_end_to_end() {
        let mut settings = ReferralSettings {
            partner: Some("binance".into()),
            ..Default::default()
        };
        assert!(settings.active_link().is_none(), "no ID yet — nothing to build");

        settings.ids.insert("binance".into(), "AB12CD".into());
        assert_eq!(
            settings.active_link().unwrap().unwrap(),
            "https://accounts.binance.com/register?ref=AB12CD"
        );

        settings
            .templates
            .insert("binance".into(), "https://www.binance.com/en/register?ref={id}".into());
        assert_eq!(
            settings.active_link().unwrap().unwrap(),
            "https://www.binance.com/en/register?ref=AB12CD"
        );
    }

    #[test]
    fn an_empty_id_is_not_an_id() {
        let mut settings = ReferralSettings {
            partner: Some("changenow".into()),
            ..Default::default()
        };
        settings.ids.insert("changenow".into(), String::new());
        assert!(settings.active_id().is_none(), "a blank field must not build a link");
    }
}
