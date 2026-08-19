use super::hub::FuturesState;
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
}
