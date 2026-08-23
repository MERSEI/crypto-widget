use crate::config::{Alert, AppSettings, WatchlistItem};
use crate::market::fx::FxRate;
use crate::market::{Candle, ConnectionStatus, PairInfo, TickerSnapshot};
use crate::window;
use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.config.snapshot())
}

#[tauri::command]
pub async fn get_tickers(state: State<'_, AppState>) -> Result<Vec<TickerSnapshot>, String> {
    Ok(state.hub.tickers_snapshot())
}

#[tauri::command]
pub async fn get_connection(state: State<'_, AppState>) -> Result<ConnectionStatus, String> {
    Ok(state.hub.connection_status())
}

/// Rust owns the expanded flag and the window geometry that follows from it. If the webview
/// ever reloads, the renderer's fresh store would default to collapsed and paint a pill into a
/// panel-sized window — so it asks on startup instead of assuming.
#[tauri::command]
pub async fn get_expanded(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.expanded.load(Ordering::SeqCst))
}

#[tauri::command]
pub async fn search_pairs(state: State<'_, AppState>, query: String) -> Result<Vec<PairInfo>, String> {
    state.provider.search_pairs(&query).await
}

#[tauri::command]
pub async fn get_klines(
    state: State<'_, AppState>,
    symbol: String,
    interval: String,
) -> Result<Vec<Candle>, String> {
    state.provider.klines(&symbol, &interval, 200).await
}

#[tauri::command]
pub async fn get_fx_rate(state: State<'_, AppState>) -> Result<Option<FxRate>, String> {
    Ok(state.fx.usdt_to_czk().await)
}

fn sync_ws_symbols(state: &State<'_, AppState>) {
    let symbols: Vec<String> = state
        .config
        .snapshot()
        .watchlist
        .iter()
        .map(|w| w.symbol.clone())
        .collect();
    state.ws.set_symbols(symbols);
}

#[tauri::command]
pub async fn add_watchlist_symbol(state: State<'_, AppState>, symbol: String) -> Result<(), String> {
    let symbol = symbol.to_uppercase();
    {
        let mut settings = state.config.settings.write().unwrap();
        if !settings.watchlist.iter().any(|w| w.symbol == symbol) {
            let order = settings.watchlist.len() as u32;
            settings.watchlist.push(WatchlistItem {
                symbol: symbol.clone(),
                order,
            });
        }
    }
    state.config.mark_dirty();
    sync_ws_symbols(&state);

    // Prices otherwise arrive only over the WS ticker stream, which stays silent for a pair
    // that isn't trading (TONUSDT sits in Binance's `BREAK` status, for instance) — the row
    // would show `···` forever. One REST snapshot gives every added pair a number right away.
    if let Ok(snapshots) = state.provider.poll_tickers(std::slice::from_ref(&symbol)).await {
        for snapshot in snapshots {
            state.hub.apply_ticker(snapshot);
        }
    }

    // Binance answered with the last trade of a halted pair (or with nothing): go straight to
    // the backup venues rather than making the newly added row wait for the sweep.
    if state.hub.needs_backup(&symbol) {
        if let Some(snapshot) = state.backup.fetch(&symbol).await {
            state.hub.apply_ticker(snapshot);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_watchlist_symbol(state: State<'_, AppState>, symbol: String) -> Result<(), String> {
    let symbol = symbol.to_uppercase();
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.watchlist.retain(|w| w.symbol != symbol);
        for (i, item) in settings.watchlist.iter_mut().enumerate() {
            item.order = i as u32;
        }
    }
    state.config.mark_dirty();
    sync_ws_symbols(&state);
    Ok(())
}

#[tauri::command]
pub async fn reorder_watchlist(state: State<'_, AppState>, symbols: Vec<String>) -> Result<(), String> {
    {
        let mut settings = state.config.settings.write().unwrap();
        for (i, symbol) in symbols.iter().enumerate() {
            if let Some(item) = settings.watchlist.iter_mut().find(|w| &w.symbol == symbol) {
                item.order = i as u32;
            }
        }
        settings.watchlist.sort_by_key(|w| w.order);
    }
    state.config.mark_dirty();
    Ok(())
}

#[tauri::command]
pub async fn set_display(
    state: State<'_, AppState>,
    quote: String,
    fiat: Option<String>,
) -> Result<(), String> {
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.display.quote = quote;
        settings.display.fiat = fiat;
    }
    state.config.mark_dirty();
    Ok(())
}

#[tauri::command]
pub async fn set_chart_settings(
    state: State<'_, AppState>,
    default_timeframe: String,
    kind: String,
) -> Result<(), String> {
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.chart.default_timeframe = default_timeframe;
        settings.chart.kind = kind;
    }
    state.config.mark_dirty();
    Ok(())
}

#[tauri::command]
pub async fn upsert_alert(state: State<'_, AppState>, alert: Alert) -> Result<(), String> {
    {
        let mut settings = state.config.settings.write().unwrap();
        if let Some(existing) = settings.alerts.iter_mut().find(|a| a.id == alert.id) {
            *existing = alert;
        } else {
            settings.alerts.push(alert);
        }
    }
    state.config.mark_dirty();
    Ok(())
}

#[tauri::command]
pub async fn delete_alert(state: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.alerts.retain(|a| a.id != id);
    }
    state.config.mark_dirty();
    Ok(())
}

#[tauri::command]
pub async fn set_notifications(state: State<'_, AppState>, toast: bool, sound: bool) -> Result<(), String> {
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.notifications.toast = toast;
        settings.notifications.sound = sound;
    }
    state.config.mark_dirty();
    Ok(())
}

/// Sends one synthetic "BTC moved 1%" toast through the exact code path a real spike alert
/// uses, so the notification chain (toggles → OS toast → sound) can be checked without waiting
/// for the market to actually move. Uses the live price when the hub already has one.
pub async fn fire_test_notification(app: AppHandle) -> Result<String, String> {
    const SYMBOL: &str = "BTCUSDT";

    // Scoped so no lock guard or non-Send `State` is held across the await below.
    let (cached_price, provider, notifications) = {
        let state = app.state::<AppState>();
        let cached = state
            .hub
            .tickers_snapshot()
            .into_iter()
            .find(|t| t.symbol == SYMBOL)
            .map(|t| t.price);
        (cached, state.provider.clone(), state.config.snapshot().notifications)
    };

    let price = match cached_price {
        Some(p) if p > 0.0 => p,
        _ => provider
            .poll_tickers(&[SYMBOL.to_string()])
            .await
            .ok()
            .and_then(|snapshots| snapshots.into_iter().next())
            .map(|t| t.price)
            .unwrap_or(0.0),
    };

    let alert = crate::alerts::engine::test_spike_alert(SYMBOL, 1.0, 15);
    if crate::alerts::engine::notify(&app, &alert, price, &notifications) {
        Ok(format!("Test toast sent ({SYMBOL} @ {price})"))
    } else {
        Err("Alert toasts are off — enable them in Settings".into())
    }
}

#[tauri::command]
pub async fn send_test_notification(app: AppHandle) -> Result<String, String> {
    fire_test_notification(app).await
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enabled { manager.enable() } else { manager.disable() };
    result.map_err(|e| e.to_string())?;
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.autostart = enabled;
    }
    state.config.mark_dirty();
    Ok(())
}

#[tauri::command]
pub async fn start_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn drag_ended(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    snap_to_edge(&window, state.inner());
    Ok(())
}

/// Re-docks the window to whichever edge it now sits closest to, storing the result as a
/// resolution-independent {edge, offset} pair.
fn snap_to_edge(window: &WebviewWindow, state: &AppState) {
    let (Ok(pos), Ok(scale), Ok(size)) = (
        window.outer_position(),
        window.scale_factor(),
        window.outer_size(),
    ) else {
        return;
    };
    let pos = pos.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);

    let Some((edge, offset)) =
        window::edge_offset_from_position(window, pos.x, pos.y, (size.width, size.height))
    else {
        return;
    };

    {
        let mut settings = state.config.settings.write().unwrap();
        settings.window.edge = edge;
        settings.window.offset = offset;
    }
    state.config.mark_dirty();

    let settings = state.config.snapshot();
    let _guard = state.geometry_lock.lock().unwrap();
    if state.expanded.load(Ordering::SeqCst) {
        window::snap_panel_geometry(window, &settings.window);
    } else {
        window::snap_pill_geometry(window, &settings.window);
    }
}

/// Debounced edge snap, driven by `WindowEvent::Moved`.
///
/// `start_dragging()` hands the pointer to the OS move loop, which swallows the `pointerup`
/// the renderer was waiting on to call `drag_ended` — so in practice the widget was dropped
/// wherever the mouse let go and never re-docked or persisted its position. Watching `Moved`
/// instead catches the end of every move regardless of who initiated it; the generation
/// counter makes sure only the last event in a burst does the work.
pub fn schedule_edge_snap(app: &AppHandle, window: &WebviewWindow) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let generation = state.move_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    let window = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        if state.move_gen.load(Ordering::SeqCst) != generation {
            return; // a later move superseded this one
        }
        snap_to_edge(&window, state.inner());
    });
}

/// Pin only suppresses blur-autocollapse. The window is always-on-top regardless.
#[tauri::command]
pub async fn set_pin(window: WebviewWindow, pinned: bool) -> Result<(), String> {
    set_pinned_internal(window.app_handle(), pinned);
    Ok(())
}

#[tauri::command]
pub async fn toggle_expand(window: WebviewWindow, state: State<'_, AppState>) -> Result<bool, String> {
    let now_expanded = !state.expanded.load(Ordering::SeqCst);
    apply_expanded(window.app_handle(), &window, state.inner(), now_expanded);
    Ok(now_expanded)
}

#[tauri::command]
pub async fn collapse_if_unpinned(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    if state.config.snapshot().window.pinned {
        return Ok(());
    }
    if state.expanded.load(Ordering::SeqCst) {
        apply_expanded(window.app_handle(), &window, state.inner(), false);
    }
    Ok(())
}

/// The single place that flips the expanded flag. The renderer keeps its own mirror of this
/// state, so every path that changes it — pill click, tray click, blur autocollapse — has to
/// emit `expanded`; otherwise the window resizes but React keeps rendering the other layout
/// (panel content squeezed into a 28px pill, or a pill floating in a 380px window).
///
/// Serialised on `state.geometry_lock` against `snap_to_edge`/every other caller of this
/// function: a click-to-expand landing mid-flight of a blur-triggered collapse (or vice versa)
/// used to let two geometry mutations run concurrently, so whichever `set_size` applied last to
/// the OS window did not necessarily match the last `state.expanded` write the renderer saw —
/// content and frame could end up on different sides of the toggle.
fn apply_expanded(app: &AppHandle, window: &WebviewWindow, state: &AppState, expanded: bool) {
    let _guard = state.geometry_lock.lock().unwrap();
    state.expanded.store(expanded, Ordering::SeqCst);
    let settings = state.config.snapshot();
    if expanded {
        window::apply_panel_geometry(window, &settings.window);
        let _ = window.set_focus();
    } else {
        window::apply_pill_geometry(window, &settings.window);
    }
    let _ = app.emit("expanded", expanded);
}

/// Persists the pin flag and mirrors it to the renderer, so the tray menu and the panel's
/// 📌 button can't drift apart.
pub fn set_pinned_internal(app: &AppHandle, pinned: bool) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.window.pinned = pinned;
    }
    state.config.mark_dirty();
    let _ = app.emit("pinned", pinned);
}

/// Same logic as `toggle_expand`, callable synchronously from the tray icon click handler.
pub fn toggle_expand_internal(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let now_expanded = !state.expanded.load(Ordering::SeqCst);
    apply_expanded(app, &window, state.inner(), now_expanded);
}

/// Expands unconditionally (tray → Settings): opening the settings pane is meaningless while
/// the panel is collapsed, since the renderer only mounts it inside the expanded layout.
pub fn expand_internal(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.expanded.load(Ordering::SeqCst) {
        let _ = window.set_focus();
    } else {
        apply_expanded(app, &window, state.inner(), true);
    }
}

/// Same logic as `collapse_if_unpinned`, callable synchronously from a window-event handler
/// (which has an `&AppHandle`, not a resolved `State` extractor).
pub fn collapse_if_unpinned_internal(app: &AppHandle, window: &WebviewWindow) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.config.snapshot().window.pinned {
        return;
    }
    if cursor_is_over(app, window) {
        return;
    }
    // Opening the futures terminal takes focus away from the panel, which is exactly the shape
    // of a click-away — and collapsing on it would shut the panel the moment its own window
    // appeared. Focus moving to another window of this app is not the user leaving.
    if another_window_of_ours_has_focus(app, window) {
        return;
    }
    if state.expanded.load(Ordering::SeqCst) {
        apply_expanded(app, window, state.inner(), false);
    }
}

/// True when one of the app's other windows currently holds focus.
fn another_window_of_ours_has_focus(app: &AppHandle, window: &WebviewWindow) -> bool {
    app.webview_windows()
        .values()
        .any(|other| other.label() != window.label() && other.is_focused().unwrap_or(false))
}

/// True when the OS cursor sits inside the window's bounds.
///
/// WebView2 hands focus to child HWNDs of its own when the page creates new native surfaces —
/// the chart canvas does exactly that — and the parent window reports itself unfocused for as
/// long as that lasts. A blur arriving while the user is still pointing at the widget is that,
/// not a click-away, and collapsing on it yanked the panel shut the moment a chart opened.
fn cursor_is_over(app: &AppHandle, window: &WebviewWindow) -> bool {
    let (Ok(cursor), Ok(pos), Ok(size)) = (
        app.cursor_position(),
        window.outer_position(),
        window.outer_size(),
    ) else {
        return false;
    };
    let right = pos.x as f64 + size.width as f64;
    let bottom = pos.y as f64 + size.height as f64;
    cursor.x >= pos.x as f64 && cursor.x <= right && cursor.y >= pos.y as f64 && cursor.y <= bottom
}
