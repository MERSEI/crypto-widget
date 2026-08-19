//! Binance USDⓈ-M futures: private account access.
//!
//! Everything in here needs an API key, which makes it the first part of the app that holds a
//! secret. Two rules follow from that and are enforced structurally, not by convention:
//!
//! - The key and secret live in the OS credential store (`keys.rs`), never in `settings.json`
//!   and never in a struct that crosses IPC. `FuturesSettings` — the half that *is* persisted
//!   and mirrored to the renderer — deliberately holds no secret material.
//! - Order placement is refused unless the venue is the testnet. That check lives in the
//!   command layer, so a renderer that sets every flag it can reach still cannot spend real
//!   money.

pub mod binance;
pub mod commands;
pub mod hub;
pub mod signer;
pub mod venue;

use serde::{Deserialize, Serialize};

/// Which Binance futures endpoint the app talks to.
///
/// `Mainnet` is read-only by construction: the app never sends an order to it, and the README
/// tells users to create the mainnet key without the Futures trading permission as a second
/// line of defence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VenueMode {
    #[default]
    Mainnet,
    Testnet,
}

impl VenueMode {
    /// Base URL for the REST API. Same paths on both, so one client serves either.
    pub fn base_url(self) -> &'static str {
        match self {
            VenueMode::Mainnet => "https://fapi.binance.com",
            VenueMode::Testnet => "https://testnet.binancefuture.com",
        }
    }

    /// Whether this venue may receive orders. Only the testnet may.
    pub fn allows_trading(self) -> bool {
        matches!(self, VenueMode::Testnet)
    }

    /// Credential-store namespace, so mainnet and testnet keys never overwrite each other.
    pub fn keyring_namespace(self) -> &'static str {
        match self {
            VenueMode::Mainnet => "futures-mainnet",
            VenueMode::Testnet => "futures-testnet",
        }
    }
}

/// The persisted, non-secret half of the futures feature.
///
/// Serialized into `settings.json` and handed to the renderer by `get_settings` — so anything
/// added here is public to the webview. Keys go to `keys.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesSettings {
    /// Master switch. Off means no key is read, no request is signed, no socket is opened —
    /// the widget behaves exactly like it did before this feature existed.
    pub enabled: bool,
    pub mode: VenueMode,
    /// Leverage prefilled in the order form. Not applied to the account until an order is sent.
    pub default_leverage: u32,
    /// Second confirmation step before an order leaves the app.
    pub confirm_orders: bool,
}

impl Default for FuturesSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: VenueMode::Mainnet,
            default_leverage: 5,
            confirm_orders: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_testnet_accepts_orders() {
        assert!(!VenueMode::Mainnet.allows_trading());
        assert!(VenueMode::Testnet.allows_trading());
    }

    #[test]
    fn the_two_venues_never_share_a_credential_slot() {
        assert_ne!(
            VenueMode::Mainnet.keyring_namespace(),
            VenueMode::Testnet.keyring_namespace()
        );
    }

    #[test]
    fn settings_default_to_the_feature_being_off() {
        let settings = FuturesSettings::default();
        assert!(!settings.enabled, "futures must stay opt-in");
        assert!(settings.confirm_orders);
    }

    #[test]
    fn settings_round_trip_as_camel_case() {
        let json = serde_json::to_string(&FuturesSettings::default()).unwrap();
        assert!(json.contains("defaultLeverage"), "{json}");
        assert!(json.contains("\"mode\":\"mainnet\""), "{json}");
        let parsed: FuturesSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, VenueMode::Mainnet);
    }
}
