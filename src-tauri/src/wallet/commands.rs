//! What the renderer may ask the wallet to do.
//!
//! Every command here re-reads the settings and the seed from their owners rather than trusting
//! anything the caller passes about them: the renderer names an amount and a destination, never
//! a key, a derivation path, or a set of token decimals. That is the whole point of the split —
//! a renderer bug can waste a request, not funds.
//!
//! The send path is deliberately two commands, `quote_wallet_transfer` then
//! `send_wallet_transfer`. The user approves a fee they were shown; `send` re-quotes and refuses
//! if the cost has run away from what was approved, so a gas spike between the dialog and the
//! click cannot turn a two-dollar fee into a fifty-dollar one behind their back.

use super::{chain, etherscan, keystore, TokenEntry, WalletSettings};
use crate::secrets::{self, CredentialStatus};
use crate::AppState;
use alloy::primitives::{Address, U256};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

/// How far above the approved figure a re-quote may land before the send is refused and the
/// user is asked again. Gas moves between the dialog and the click, so an exact match would
/// refuse honest transfers all day; a quarter over is noise, anything beyond it is a different
/// decision than the one the user made.
const FEE_DRIFT_TOLERANCE_PERCENT: u64 = 25;

/// History rows are capped so a wallet with years of activity cannot ask for a response that
/// takes longer to parse than the user is willing to wait.
const MAX_HISTORY: u32 = 100;

/// Label of the standalone wallet window. Like the futures terminal, it is deliberately not
/// `"main"`: none of the pill's docking, always-on-top or auto-collapse behaviour applies to a
/// window someone types an address into.
pub const WALLET_WINDOW: &str = "wallet";

/// Everything the wallet screen needs to render itself, in one round trip: whether a wallet
/// exists, which address is active, and what is configured around it. Contains no secret —
/// `status` carries an address, and `etherscan` a masked key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletState {
    pub status: keystore::WalletStatus,
    pub settings: WalletSettings,
    pub etherscan: CredentialStatus,
}

/// A freshly generated wallet. The phrase crosses the boundary exactly once, right here, so the
/// user can write it down — see `keystore::reveal_phrase` for why that exception exists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewWallet {
    pub phrase: String,
    pub state: WalletState,
}

/// One shared client for the history endpoint. The free Etherscan tier budgets by key, not by
/// connection, so a client per call would buy nothing and pay for a fresh TLS handshake each
/// time.
fn history_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default()
    })
}

fn wallet_settings(state: &State<'_, AppState>) -> WalletSettings {
    state.config.snapshot().wallet
}

/// Mutates the wallet section and returns the state the renderer should now show.
///
/// The lock is dropped before `keystore::status` runs: that call reaches the OS credential store
/// and derives a key, which is slow enough that holding a write lock over it would stall the
/// autosave loop and every other command.
fn update(
    state: &State<'_, AppState>,
    mutate: impl FnOnce(&mut WalletSettings) -> Result<(), String>,
) -> Result<WalletState, String> {
    let settings = {
        let mut guard = state.config.settings.write().unwrap();
        mutate(&mut guard.wallet)?;
        guard.wallet.clone()
    };
    state.config.mark_dirty();
    Ok(state_of(&settings))
}

fn state_of(settings: &WalletSettings) -> WalletState {
    WalletState {
        status: keystore::status(settings.account_index),
        settings: settings.clone(),
        etherscan: secrets::status_raw(etherscan::NAMESPACE),
    }
}

/// Parses a destination and refuses the two addresses that are always a mistake.
///
/// The zero address is a burn, and the token contract itself is where tokens go to be lost —
/// both are things a user does by accident and never by intent, and neither is reversible.
fn parse_destination(value: &str, token: Option<Address>) -> Result<Address, String> {
    let address = chain::parse_address(value)?;
    if address == Address::ZERO {
        return Err("that is the zero address — anything sent there is burned".into());
    }
    if token == Some(address) {
        return Err(
            "that is the token's own contract, not a wallet — funds sent there are lost".into(),
        );
    }
    Ok(address)
}

fn parse_token(contract: Option<String>) -> Result<Option<Address>, String> {
    match contract {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => Ok(Some(chain::parse_address(&value)?)),
    }
}

#[tauri::command]
pub async fn get_wallet_state(state: State<'_, AppState>) -> Result<WalletState, String> {
    Ok(state_of(&wallet_settings(&state)))
}

/// Creates a wallet from fresh OS entropy and stores it. Refuses to run over an existing
/// wallet: overwriting a seed the user has funds under, without them typing anything that means
/// "yes, destroy it", is unrecoverable.
#[tauri::command]
pub async fn create_wallet(state: State<'_, AppState>) -> Result<NewWallet, String> {
    if keystore::has_phrase() {
        return Err("a wallet already exists — remove it first if you mean to replace it".into());
    }
    let phrase = keystore::generate_phrase()?;
    keystore::save_phrase(&phrase)?;
    let state = update(&state, |wallet| {
        wallet.account_index = 0;
        Ok(())
    })?;
    Ok(NewWallet { phrase, state })
}

/// Restores a wallet from a phrase the user brings. Same overwrite rule as `create_wallet`.
#[tauri::command]
pub async fn import_wallet(
    state: State<'_, AppState>,
    phrase: String,
) -> Result<WalletState, String> {
    if keystore::has_phrase() {
        return Err("a wallet already exists — remove it first if you mean to replace it".into());
    }
    keystore::save_phrase(&phrase)?;
    update(&state, |wallet| {
        wallet.account_index = 0;
        Ok(())
    })
}

/// Hands the phrase back so the user can write it down. Called only from an explicit "reveal"
/// action, never while rendering a screen.
#[tauri::command]
pub async fn reveal_seed_phrase() -> Result<String, String> {
    keystore::reveal_phrase()
}

/// Deletes the seed from the credential store.
///
/// The token list and RPC settings survive on purpose: they are not secret, and a user who
/// removes a wallet from this machine and restores it later would otherwise have to re-add every
/// token by hand.
#[tauri::command]
pub async fn forget_wallet(state: State<'_, AppState>) -> Result<WalletState, String> {
    keystore::delete_phrase()?;
    Ok(state_of(&wallet_settings(&state)))
}

/// Switches the active BIP-44 account.
#[tauri::command]
pub async fn set_wallet_account(
    state: State<'_, AppState>,
    index: u32,
) -> Result<WalletState, String> {
    // A four-figure index is already far past what anyone navigates to by hand, and every one
    // of them costs a key derivation to display.
    if index > 255 {
        return Err("account index must be between 0 and 255".into());
    }
    update(&state, |wallet| {
        wallet.account_index = index;
        Ok(())
    })
}

/// Points the wallet at a different node or chain. `native_symbol` names the chain's gas token
/// ("ETH", "POL", "BNB", …) — there is no RPC call that answers this, so the caller (a network
/// preset, or a manual entry defaulting to "ETH") must supply it.
#[tauri::command]
pub async fn set_wallet_network(
    state: State<'_, AppState>,
    rpc_url: String,
    chain_id: u64,
    native_symbol: String,
) -> Result<WalletState, String> {
    let url = rpc_url.trim().to_string();
    // Plain http is refused rather than merely discouraged: an RPC carries the addresses being
    // queried and the raw signed transactions being broadcast.
    if !url.starts_with("https://") {
        return Err("the RPC URL must start with https://".into());
    }
    if chain_id == 0 {
        return Err("chain id must be greater than zero".into());
    }
    let symbol = native_symbol.trim().to_string();
    if symbol.is_empty() {
        return Err("the native currency symbol cannot be empty".into());
    }
    update(&state, |wallet| {
        wallet.rpc_url = url;
        wallet.chain_id = chain_id;
        wallet.native_symbol = symbol;
        Ok(())
    })
}

/// Shows or hides the floating price pill. The wallet keeps working either way — the pill is a
/// view, not the app.
#[tauri::command]
pub async fn set_wallet_widget_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<WalletState, String> {
    let next = update(&state, |wallet| {
        wallet.widget_enabled = enabled;
        Ok(())
    })?;
    // Applied to the live window as well as to the file: a toggle that only took effect after a
    // restart would read as broken, and the user would click it again.
    if let Some(pill) = app.get_webview_window("main") {
        let _ = if enabled { pill.show() } else { pill.hide() };
    }
    Ok(next)
}

/// Adds an ERC-20 to the portfolio view, reading its symbol and decimals from the contract.
///
/// The metadata is never taken from the caller: a wrong `decimals` here would misprice every
/// balance the token ever shows, and the contract is the only authority on it.
#[tauri::command]
pub async fn add_wallet_token(
    state: State<'_, AppState>,
    address: String,
) -> Result<WalletState, String> {
    let settings = wallet_settings(&state);
    let contract = chain::parse_address(&address)?;
    let checksummed = contract.to_checksum(None);

    if settings
        .tokens
        .iter()
        .any(|t| t.address.eq_ignore_ascii_case(&checksummed))
    {
        return Err("that token is already on the list".into());
    }

    // Reading a balance is also the cheapest proof that this address is an ERC-20 at all: a
    // plain wallet address, or a contract that lives on another chain, fails here rather than
    // sitting in the list as a row that never loads.
    let owner = keystore::address_for(settings.account_index)?;
    let balance =
        chain::token_balance(&settings.rpc_url, chain::parse_address(&owner)?, contract).await?;

    update(&state, |wallet| {
        wallet.tokens.push(TokenEntry {
            address: checksummed,
            symbol: balance.symbol.clone(),
            decimals: balance.decimals,
        });
        Ok(())
    })
}

#[tauri::command]
pub async fn remove_wallet_token(
    state: State<'_, AppState>,
    address: String,
) -> Result<WalletState, String> {
    update(&state, |wallet| {
        wallet
            .tokens
            .retain(|t| !t.address.eq_ignore_ascii_case(address.trim()));
        Ok(())
    })
}

/// The native balance plus one row per configured token.
///
/// A token that fails to read becomes a row carrying its error rather than disappearing or
/// showing a zero: a silent `0` next to a token the user knows they hold is indistinguishable
/// from "your funds are gone".
#[tauri::command]
pub async fn get_wallet_balances(
    state: State<'_, AppState>,
) -> Result<Vec<chain::AssetBalance>, String> {
    let settings = wallet_settings(&state);
    let owner = chain::parse_address(&keystore::address_for(settings.account_index)?)?;

    let mut balances =
        vec![chain::native_balance(&settings.rpc_url, owner, &settings.native_symbol).await?];

    for token in &settings.tokens {
        let row = match chain::parse_address(&token.address) {
            Ok(contract) => chain::token_balance(&settings.rpc_url, owner, contract).await,
            Err(e) => Err(e),
        };
        balances.push(row.unwrap_or_else(|e| chain::AssetBalance {
            contract: Some(token.address.clone()),
            symbol: token.symbol.clone(),
            decimals: token.decimals,
            raw: "0".into(),
            amount: "—".into(),
            error: Some(e),
        }));
    }

    Ok(balances)
}

/// Transaction history for the active account. Requires an Etherscan key: there is no public
/// endpoint that serves this, and pretending otherwise would show an empty list to a wallet
/// with plenty of history.
#[tauri::command]
pub async fn get_wallet_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<etherscan::Transfer>, String> {
    let settings = wallet_settings(&state);
    let address = keystore::address_for(settings.account_index)?;
    let api_key = secrets::load_raw(etherscan::NAMESPACE).ok_or_else(|| {
        "add an Etherscan API key in settings to see transaction history".to_string()
    })?;

    let contracts: Vec<String> = settings.tokens.iter().map(|t| t.address.clone()).collect();
    let limit = limit.unwrap_or(25).clamp(1, MAX_HISTORY);

    etherscan::fetch_history(
        history_client(),
        settings.chain_id,
        &api_key,
        &address,
        &contracts,
        limit,
        &settings.native_symbol,
    )
    .await
    // The kind is what tells the user which of these is their problem to fix: a rejected key is
    // a settings change, a rate limit is a matter of waiting, and neither should read like the
    // wallet is broken.
    .map_err(|e| match e.kind {
        etherscan::ErrorKind::Config => {
            format!("{} — check the Etherscan API key in settings", e.message)
        }
        etherscan::ErrorKind::RateLimit => {
            format!("{} — the free Etherscan tier allows 5 requests a second", e.message)
        }
        etherscan::ErrorKind::Upstream | etherscan::ErrorKind::Network => e.message,
    })
}

#[tauri::command]
pub async fn set_etherscan_key(
    state: State<'_, AppState>,
    api_key: String,
) -> Result<WalletState, String> {
    secrets::save_raw(etherscan::NAMESPACE, &api_key)?;
    Ok(state_of(&wallet_settings(&state)))
}

#[tauri::command]
pub async fn clear_etherscan_key(state: State<'_, AppState>) -> Result<WalletState, String> {
    secrets::delete(etherscan::NAMESPACE)?;
    Ok(state_of(&wallet_settings(&state)))
}

/// Prices a transfer without sending it. Everything the user is about to approve is computed
/// here, on this side, from the transaction that `send_wallet_transfer` rebuilds identically.
#[tauri::command]
pub async fn quote_wallet_transfer(
    state: State<'_, AppState>,
    to: String,
    amount: String,
    contract: Option<String>,
) -> Result<chain::FeeQuote, String> {
    let settings = wallet_settings(&state);
    let token = parse_token(contract)?;
    let destination = parse_destination(&to, token)?;
    let from = chain::parse_address(&keystore::address_for(settings.account_index)?)?;

    chain::quote(
        &settings.rpc_url,
        from,
        destination,
        &amount,
        token,
        &settings.native_symbol,
    )
    .await
}

/// Signs and broadcasts a transfer the user approved.
///
/// `approvedMaxCostWei` is the `maxCostWei` from the quote they saw. It is re-checked against a
/// fresh quote here rather than trusted: the renderer holding a stale or tampered figure must
/// not be able to authorise a fee nobody agreed to, and an unaffordable transfer must fail
/// before it is signed rather than after the network rejects it.
#[tauri::command]
pub async fn send_wallet_transfer(
    state: State<'_, AppState>,
    to: String,
    amount: String,
    contract: Option<String>,
    approved_max_cost_wei: String,
) -> Result<String, String> {
    let settings = wallet_settings(&state);
    let token = parse_token(contract)?;
    let destination = parse_destination(&to, token)?;
    let signer = keystore::signer_for(settings.account_index)?;
    let from = signer.address();

    let approved = U256::from_str_radix(approved_max_cost_wei.trim(), 10)
        .map_err(|_| "the approved fee is unreadable — review the transfer again".to_string())?;

    let quote = chain::quote(
        &settings.rpc_url,
        from,
        destination,
        &amount,
        token,
        &settings.native_symbol,
    )
    .await?;
    if !quote.affordable {
        return Err(quote
            .shortfall
            .unwrap_or_else(|| "the account cannot cover this transfer".into()));
    }

    let current = U256::from_str_radix(&quote.max_cost_wei, 10)
        .map_err(|_| "could not read the current fee estimate".to_string())?;
    if current > fee_ceiling(approved) {
        return Err(format!(
            "the fee rose to {} {} since you approved it — review the transfer again",
            quote.max_cost_eth, settings.native_symbol
        ));
    }

    chain::send(&settings.rpc_url, signer, destination, &amount, token).await
}

/// Opens (or focuses) the wallet window.
#[tauri::command]
pub async fn open_wallet_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WALLET_WINDOW) {
        let _ = window.unminimize();
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        WALLET_WINDOW,
        // The renderer picks its root component off this query parameter.
        tauri::WebviewUrl::App("index.html?window=wallet".into()),
    )
    .title("crypto-widget — wallet")
    .inner_size(820.0, 620.0)
    .min_inner_size(520.0, 420.0)
    .resizable(true)
    .decorations(true)
    .center()
    .build()
    .map_err(|e| format!("could not open the wallet window: {e}"))?;

    Ok(())
}

/// The highest fee an approval of `approved` still covers.
fn fee_ceiling(approved: U256) -> U256 {
    approved + approved * U256::from(FEE_DRIFT_TOLERANCE_PERCENT) / U256::from(100u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
    const WALLET: &str = "0x9858EfFD232B4033E47d90003D41EC34EcaEda94";

    #[test]
    fn the_zero_address_is_never_a_destination() {
        let err =
            parse_destination("0x0000000000000000000000000000000000000000", None).unwrap_err();
        assert!(err.contains("burned"), "{err}");
    }

    #[test]
    fn sending_a_token_to_its_own_contract_is_refused() {
        let token = chain::parse_address(TOKEN).unwrap();
        let err = parse_destination(TOKEN, Some(token)).unwrap_err();
        assert!(err.contains("contract"), "{err}");
        // The same address is a perfectly ordinary destination for a native transfer, so the
        // rule may not fire when no token is involved.
        assert!(parse_destination(TOKEN, None).is_ok());
    }

    #[test]
    fn an_ordinary_destination_survives_both_checks() {
        let token = chain::parse_address(TOKEN).unwrap();
        assert_eq!(
            parse_destination(WALLET, Some(token))
                .unwrap()
                .to_checksum(None),
            WALLET
        );
    }

    #[test]
    fn a_missing_or_blank_contract_means_the_native_asset() {
        assert_eq!(parse_token(None).unwrap(), None);
        assert_eq!(parse_token(Some("   ".into())).unwrap(), None);
        assert!(parse_token(Some(TOKEN.into())).unwrap().is_some());
        assert!(parse_token(Some("0xnope".into())).is_err());
    }

    /// The drift check is what stands between "you approved a $2 fee" and "you paid $50".
    #[test]
    fn the_fee_ceiling_allows_drift_but_not_a_spike() {
        let approved = U256::from(1_000_000u64);
        assert_eq!(fee_ceiling(approved), U256::from(1_250_000u64));
        assert!(U256::from(1_200_000u64) <= fee_ceiling(approved));
        assert!(U256::from(1_300_000u64) > fee_ceiling(approved));
    }

    /// A quote of zero would otherwise make the ceiling zero too, refusing every send with a
    /// confusing "the fee rose" message.
    #[test]
    fn a_zero_approval_does_not_underflow_or_overflow() {
        assert_eq!(fee_ceiling(U256::ZERO), U256::ZERO);
    }
}
