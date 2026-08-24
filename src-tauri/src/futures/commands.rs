use super::hub::FuturesState;
use super::venue::{NewOrder, OrderRecord, OrderReceipt, OrderSide, OrderType};
use super::VenueMode;
use crate::secrets::{self, Credential, CredentialStatus};
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

/// Label of the standalone futures window. Everything window-related in `window.rs` and the
/// event handlers in `lib.rs` is scoped to `"main"`, so this window inherits none of the pill's
/// docking, always-on-top, or auto-collapse behaviour — which is the point of it being separate.
pub const FUTURES_WINDOW: &str = "futures";

/// Key status for both venues at once, so the settings form can show which one is configured
/// without asking twice — and without either answer containing a secret.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesKeyStatus {
    pub mainnet: CredentialStatus,
    pub testnet: CredentialStatus,
}

fn mode_from(name: &str) -> Result<VenueMode, String> {
    match name {
        "mainnet" => Ok(VenueMode::Mainnet),
        "testnet" => Ok(VenueMode::Testnet),
        other => Err(format!("unknown venue: {other}")),
    }
}

fn side_from(value: &str) -> Result<OrderSide, String> {
    match value {
        "buy" => Ok(OrderSide::Buy),
        "sell" => Ok(OrderSide::Sell),
        other => Err(format!("unknown order side: {other}")),
    }
}

fn order_type_from(value: &str) -> Result<OrderType, String> {
    match value {
        "market" => Ok(OrderType::Market),
        "limit" => Ok(OrderType::Limit),
        other => Err(format!("unknown order type: {other}")),
    }
}

/// Upper-cases and validates a symbol before it reaches a signed request. Binance rejects a bad
/// symbol anyway, but doing it here turns "BTC/USDT" or an empty field into a message about the
/// symbol instead of an opaque exchange error.
fn normalize_symbol(symbol: &str) -> Result<String, String> {
    let trimmed = symbol.trim().to_uppercase();
    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("symbol must be letters and digits only, e.g. BTCUSDT".into());
    }
    Ok(trimmed)
}

#[tauri::command]
pub async fn get_futures_state(state: State<'_, AppState>) -> Result<FuturesState, String> {
    Ok(state.futures.state())
}

#[tauri::command]
pub async fn get_futures_keys() -> Result<FuturesKeyStatus, String> {
    Ok(FuturesKeyStatus {
        mainnet: secrets::status(VenueMode::Mainnet.keyring_namespace()),
        testnet: secrets::status(VenueMode::Testnet.keyring_namespace()),
    })
}

/// Stores an API key pair in the OS credential store.
///
/// The secret travels renderer → backend exactly once, here, and is never sent back: every
/// later read happens inside Rust. Returns the masked status so the form can render what it
/// just saved without holding the value.
#[tauri::command]
pub async fn set_futures_keys(
    state: State<'_, AppState>,
    venue: String,
    api_key: String,
    api_secret: String,
) -> Result<FuturesKeyStatus, String> {
    let mode = mode_from(&venue)?;
    secrets::save(
        mode.keyring_namespace(),
        &Credential {
            key: api_key,
            secret: api_secret,
        },
    )?;
    state.futures.reload();
    get_futures_keys().await
}

#[tauri::command]
pub async fn clear_futures_keys(
    state: State<'_, AppState>,
    venue: String,
) -> Result<FuturesKeyStatus, String> {
    let mode = mode_from(&venue)?;
    secrets::delete(mode.keyring_namespace())?;
    state.futures.reload();
    get_futures_keys().await
}

#[tauri::command]
pub async fn set_futures_enabled(state: State<'_, AppState>, enabled: bool) -> Result<FuturesState, String> {
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.futures.enabled = enabled;
    }
    state.config.mark_dirty();
    state.futures.reload();
    Ok(state.futures.state())
}

#[tauri::command]
pub async fn set_futures_venue(state: State<'_, AppState>, venue: String) -> Result<FuturesState, String> {
    let mode = mode_from(&venue)?;
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.futures.mode = mode;
    }
    state.config.mark_dirty();
    state.futures.reload();
    Ok(state.futures.state())
}

#[tauri::command]
pub async fn set_futures_preferences(
    state: State<'_, AppState>,
    default_leverage: u32,
    confirm_orders: bool,
) -> Result<FuturesState, String> {
    if !(1..=125).contains(&default_leverage) {
        return Err("leverage must be between 1 and 125".into());
    }
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.futures.default_leverage = default_leverage;
        settings.futures.confirm_orders = confirm_orders;
    }
    state.config.mark_dirty();
    Ok(state.futures.state())
}

/// Forces a refresh now instead of waiting for the next poll.
#[tauri::command]
pub async fn refresh_futures(state: State<'_, AppState>) -> Result<FuturesState, String> {
    // The state carries the failure; the panel reads it from there rather than from a rejected
    // promise, so an error and a stale snapshot can be shown together.
    let _ = state.futures.refresh().await;
    Ok(state.futures.state())
}

#[tauri::command]
pub async fn test_futures_connection(state: State<'_, AppState>) -> Result<String, String> {
    state.futures.check_credentials().await
}

/// Places a market or limit order. `FuturesHub::place_order` refuses this outright unless the
/// active venue is the testnet — validation here is about the shape of the request, not the
/// permission to send it.
#[tauri::command]
pub async fn place_futures_order(
    state: State<'_, AppState>,
    symbol: String,
    side: String,
    order_type: String,
    quantity: f64,
    price: Option<f64>,
    reduce_only: bool,
) -> Result<OrderReceipt, String> {
    let symbol = normalize_symbol(&symbol)?;
    let side = side_from(&side)?;
    let order_type = order_type_from(&order_type)?;
    if !(quantity.is_finite() && quantity > 0.0) {
        return Err("quantity must be a positive number".into());
    }
    let price = match order_type {
        OrderType::Limit => {
            let p = price.ok_or("a limit order needs a price")?;
            if !(p.is_finite() && p > 0.0) {
                return Err("price must be a positive number".into());
            }
            Some(p)
        }
        OrderType::Market => None,
    };

    state
        .futures
        .place_order(NewOrder {
            symbol,
            side,
            order_type,
            quantity,
            price,
            reduce_only,
        })
        .await
}

/// Closes (`quantity: null`) or partially reduces (`quantity` given) an open position with a
/// reduce-only market order. `side` is the *closing* order's side — `sell` to close a long,
/// `buy` to close a short — named by the caller, which already knows the position's direction
/// from the snapshot it is looking at.
#[tauri::command]
pub async fn close_futures_position(
    state: State<'_, AppState>,
    symbol: String,
    side: String,
    quantity: Option<f64>,
) -> Result<OrderReceipt, String> {
    let symbol = normalize_symbol(&symbol)?;
    let side = side_from(&side)?;
    if let Some(q) = quantity {
        if !(q.is_finite() && q > 0.0) {
            return Err("quantity must be a positive number".into());
        }
    }
    state.futures.close_position(symbol, side, quantity).await
}

/// Cancels a resting (unfilled) order.
#[tauri::command]
pub async fn cancel_futures_order(
    state: State<'_, AppState>,
    symbol: String,
    order_id: i64,
) -> Result<(), String> {
    let symbol = normalize_symbol(&symbol)?;
    state.futures.cancel_order(symbol, order_id).await
}

#[tauri::command]
pub async fn set_futures_leverage(
    state: State<'_, AppState>,
    symbol: String,
    leverage: u32,
) -> Result<(), String> {
    let symbol = normalize_symbol(&symbol)?;
    if !(1..=125).contains(&leverage) {
        return Err("leverage must be between 1 and 125".into());
    }
    state.futures.set_leverage(symbol, leverage).await
}

/// Switches a symbol between isolated and cross margin. Binance refuses this while a position or
/// an open order exists on the symbol — that refusal reaches the caller as this command's error.
#[tauri::command]
pub async fn set_futures_margin_type(
    state: State<'_, AppState>,
    symbol: String,
    isolated: bool,
) -> Result<(), String> {
    let symbol = normalize_symbol(&symbol)?;
    state.futures.set_margin_type(symbol, isolated).await
}

/// Order history for one symbol. Read-only, so it works on mainnet too.
#[tauri::command]
pub async fn get_futures_order_history(
    state: State<'_, AppState>,
    symbol: String,
    limit: Option<u32>,
) -> Result<Vec<OrderRecord>, String> {
    let symbol = normalize_symbol(&symbol)?;
    let limit = limit.unwrap_or(50).clamp(1, 200);
    state.futures.order_history(symbol, limit).await
}

/// Opens (or focuses) the standalone futures terminal.
///
/// A 380px-wide docked pill cannot hold an order form and a position table at once, so the
/// full view gets a normal, resizable, decorated window — everything the widget itself
/// deliberately is not.
#[tauri::command]
pub async fn open_futures_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FUTURES_WINDOW) {
        let _ = window.unminimize();
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        FUTURES_WINDOW,
        // The renderer picks its root component off this query parameter.
        tauri::WebviewUrl::App("index.html?window=futures".into()),
    )
    .title("crypto-widget — futures")
    .inner_size(900.0, 600.0)
    .min_inner_size(560.0, 360.0)
    .resizable(true)
    .decorations(true)
    .center()
    .build()
    .map_err(|e| format!("could not open the futures window: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_names_map_to_modes_and_nothing_else_does() {
        assert_eq!(mode_from("mainnet").unwrap(), VenueMode::Mainnet);
        assert_eq!(mode_from("testnet").unwrap(), VenueMode::Testnet);
        assert!(mode_from("TESTNET").is_err(), "the wire format is lowercase");
        assert!(mode_from("").is_err());
        assert!(mode_from("mainnet ").is_err());
    }

    #[test]
    fn order_side_and_type_are_lowercase_on_the_wire() {
        assert_eq!(side_from("buy").unwrap(), OrderSide::Buy);
        assert_eq!(side_from("sell").unwrap(), OrderSide::Sell);
        assert!(side_from("BUY").is_err());
        assert_eq!(order_type_from("market").unwrap(), OrderType::Market);
        assert_eq!(order_type_from("limit").unwrap(), OrderType::Limit);
        assert!(order_type_from("stop").is_err());
    }

    #[test]
    fn a_symbol_is_upper_cased_and_checked_for_junk() {
        assert_eq!(normalize_symbol(" btcusdt ").unwrap(), "BTCUSDT");
        assert!(normalize_symbol("BTC/USDT").is_err(), "a slash is not a symbol character");
        assert!(normalize_symbol("").is_err());
        assert!(normalize_symbol("   ").is_err());
    }
}
