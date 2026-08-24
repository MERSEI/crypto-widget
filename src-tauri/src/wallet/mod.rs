//! The wallet: balances, history, and the one feature in this app that can spend money.
//!
//! Structurally this follows the rule the rest of the app already lives by — Rust owns the
//! state, React renders it — but here it is load-bearing rather than tidy. The seed phrase
//! never crosses the IPC boundary, so a compromised or merely buggy renderer cannot sign
//! anything: it can only ask this layer to, and this layer re-validates every argument.
//!
//! What the renderer *can* do is ask for a transfer. Three checks stand between that request
//! and a broadcast, and all three live on this side:
//!
//! 1. the destination is parsed and its EIP-55 checksum verified (`chain::parse_address`);
//! 2. the amount is parsed against the asset's real decimals, read from the contract;
//! 3. the user has approved a fee quote derived from the very same transaction that is sent.

pub mod chain;
pub mod commands;
pub mod etherscan;
pub mod keystore;

use serde::{Deserialize, Serialize};

/// A public RPC that answers without an API key. Chosen because the alternatives commonly
/// suggested for this — `cloudflare-eth.com` and `eth.llamarpc.com` — refuse requests from a
/// desktop client, which looks exactly like "the network is down" from inside the app.
pub const DEFAULT_RPC_URL: &str = "https://ethereum-rpc.publicnode.com";

/// An ERC-20 the user has added to their portfolio view.
///
/// `symbol` and `decimals` are cached here for rendering a row before its balance arrives, but
/// they are never used for arithmetic — every transfer re-reads the decimals from the contract,
/// because a wrong value here would scale the amount by orders of magnitude.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenEntry {
    pub address: String,
    pub symbol: String,
    pub decimals: u8,
}

/// The persisted, non-secret half of the wallet. Serialized into `settings.json` and handed to
/// the renderer, so nothing secret may be added here — the seed phrase lives in `keystore`.
/// `serde(default)` on the container, not just on the section in `AppSettings`: a file written
/// by an earlier build has *some* of these keys and not others, and without this a single
/// missing field fails the whole parse — which backs the settings file up and starts from
/// scratch, taking the user's watchlist and alerts with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WalletSettings {
    pub rpc_url: String,
    pub chain_id: u64,
    /// BIP-44 index of the active account.
    pub account_index: u32,
    pub tokens: Vec<TokenEntry>,
    /// Whether the floating price pill is shown. The wallet is the main window now; the pill is
    /// an extra the user opts into from settings.
    pub widget_enabled: bool,
    /// Ticker of the chain's native currency — "ETH" on mainnet and most L2s, "POL" on Polygon,
    /// "BNB" on BSC. Set alongside `rpc_url`/`chain_id` by `set_wallet_network`, never inferred:
    /// there is no RPC call that answers "what do you call your gas token".
    pub native_symbol: String,
}

impl Default for WalletSettings {
    fn default() -> Self {
        Self {
            rpc_url: DEFAULT_RPC_URL.to_string(),
            chain_id: 1,
            account_index: 0,
            tokens: Vec::new(),
            // On by default so an existing user, for whom the pill *was* the app, still finds
            // it where they left it after the update.
            widget_enabled: true,
            native_symbol: "ETH".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_ethereum_mainnet_with_a_working_rpc() {
        let settings = WalletSettings::default();
        assert_eq!(settings.chain_id, 1);
        assert!(settings.rpc_url.starts_with("https://"));
        assert_eq!(settings.account_index, 0);
        assert_eq!(settings.native_symbol, "ETH");
    }

    #[test]
    fn an_existing_user_keeps_their_pill() {
        assert!(WalletSettings::default().widget_enabled);
    }

    #[test]
    fn wallet_settings_round_trip_through_json() {
        let mut settings = WalletSettings::default();
        settings.tokens.push(TokenEntry {
            address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into(),
            symbol: "USDC".into(),
            decimals: 6,
        });
        let json = serde_json::to_string(&settings).unwrap();
        let back: WalletSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tokens, settings.tokens);
        assert_eq!(back.rpc_url, settings.rpc_url);
    }

    /// The settings file is loaded with `#[serde(default)]` on each section, so a file written
    /// before the wallet existed has to gain the defaults rather than fail to parse — a parse
    /// failure backs the file up and starts over, which would wipe the user's watchlist.
    #[test]
    fn a_settings_file_without_a_wallet_section_still_loads() {
        let back: WalletSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(back.chain_id, 1);
        assert_eq!(back.rpc_url, DEFAULT_RPC_URL);
    }

    #[test]
    fn a_half_written_wallet_section_keeps_the_rest_of_the_file() {
        // The failure mode this guards: one unknown-to-this-build key missing, the whole
        // settings.json declared corrupt, and the user's watchlist and alerts backed up and
        // replaced with defaults.
        let back: WalletSettings = serde_json::from_str(r#"{"accountIndex":3}"#).unwrap();
        assert_eq!(back.account_index, 3);
        assert_eq!(back.chain_id, 1);
        assert!(back.widget_enabled);
    }
}
