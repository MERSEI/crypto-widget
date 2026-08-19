mod alerts;
mod commands;
mod config;
mod futures;
mod market;
mod referral;
mod secrets;
mod tray;
mod window;

use config::ConfigState;
use futures::hub::FuturesHub;
use market::backup::BackupProvider;
use market::binance_rest::BinanceProvider;
use market::binance_ws::{self, WsHandle};
use market::fx::FxProvider;
use market::hub::MarketHub;
use market::provider::MarketProvider;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub config: Arc<ConfigState>,
    pub hub: Arc<MarketHub>,
    pub ws: WsHandle,
    pub provider: Arc<dyn MarketProvider>,
    pub backup: Arc<BackupProvider>,
    pub fx: FxProvider,
    /// Private futures account state. Independent of `hub`: it has its own credentials, its own
    /// rate budget, and it stays idle unless the user switched the feature on.
    pub futures: Arc<FuturesHub>,
    pub expanded: AtomicBool,
    /// Bumped on every `WindowEvent::Moved` so a debounced edge snap can tell whether it is
    /// still the most recent move.
    pub move_gen: AtomicU64,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            let config = Arc::new(ConfigState::load(&handle));
            config::spawn_autosave(config.clone());

            let hub = MarketHub::new(handle.clone(), config.clone());
            hub.clone().spawn_emit_loop();

            let provider: Arc<dyn MarketProvider> = Arc::new(BinanceProvider::new(config.cache_dir.clone()));
            let ws = binance_ws::spawn(hub.clone(), provider.clone());

            // Keeps watchlist pairs priced when Binance stops quoting them — a halted pair
            // otherwise keeps serving the price of its last trade for as long as it stays
            // halted.
            let backup = Arc::new(BackupProvider::new());
            market::backup::spawn_watcher(hub.clone(), config.clone(), backup.clone());

            let initial_symbols: Vec<String> = config
                .snapshot()
                .watchlist
                .iter()
                .map(|w| w.symbol.clone())
                .collect();
            if !initial_symbols.is_empty() {
                ws.set_symbols(initial_symbols.clone());

                // Seed from REST so the watchlist shows real numbers on the first frame instead
                // of `···` until the WS delivers its first tick — which for a halted pair is
                // never.
                let seed_hub = hub.clone();
                let seed_provider = provider.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(snapshots) = seed_provider.poll_tickers(&initial_symbols).await {
                        for snapshot in snapshots {
                            seed_hub.apply_ticker(snapshot);
                        }
                    }
                });
            }

            let fx = FxProvider::new(config.cache_dir.clone());

            let futures_hub = FuturesHub::new(handle.clone(), config.clone());
            futures_hub.clone().spawn_poll_loop();

            app.manage(AppState {
                config: config.clone(),
                hub,
                ws,
                provider,
                backup,
                fx,
                futures: futures_hub,
                expanded: AtomicBool::new(false),
                move_gen: AtomicU64::new(0),
            });

            if let Some(main_window) = app.get_webview_window("main") {
                let settings = config.snapshot();
                // Without an explicit minimum, Windows clamps the window to its system
                // minimum tracking width (~136px), which stretches the 28px pill.
                let _ = main_window.set_min_size(Some(tauri::LogicalSize::new(
                    window::PILL_WIDTH,
                    window::PILL_HEIGHT,
                )));
                window::apply_pill_geometry(&main_window, &settings.window);
                // A docked widget is useless behind other windows, so always-on-top is
                // unconditional; `pinned` only governs whether blur auto-collapses the panel.
                let _ = main_window.set_always_on_top(true);
                let _ = main_window.show();

                let event_window = main_window.clone();
                let event_app = handle.clone();
                main_window.on_window_event(move |event| match event {
                    tauri::WindowEvent::Focused(false) => {
                        // WebView2 drops window focus for a frame or two whenever the webview
                        // spins up a new native surface — opening a chart is the reliable
                        // trigger — which collapsed the panel out from under the user mid-click.
                        // Re-check before acting: a real click-away stays unfocused.
                        let app = event_app.clone();
                        let window = event_window.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(220)).await;
                            if window.is_focused().unwrap_or(false) {
                                return;
                            }
                            commands::collapse_if_unpinned_internal(&app, &window);
                        });
                    }
                    tauri::WindowEvent::Moved(_) => {
                        commands::schedule_edge_snap(&event_app, &event_window);
                    }
                    _ => {}
                });
            }

            tray::setup(&handle)?;

            // Smoke-test hook: `CRYPTO_WIDGET_TEST_NOTIFICATION=1` fires one sample toast a few
            // seconds after startup, so the notification chain can be checked from a script
            // without clicking through the tray.
            if std::env::var("CRYPTO_WIDGET_TEST_NOTIFICATION").is_ok() {
                let test_handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    match commands::fire_test_notification(test_handle).await {
                        Ok(msg) => println!("{msg}"),
                        Err(e) => eprintln!("test notification: {e}"),
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::get_tickers,
            commands::get_connection,
            commands::get_expanded,
            commands::search_pairs,
            commands::get_klines,
            commands::get_fx_rate,
            commands::add_watchlist_symbol,
            commands::remove_watchlist_symbol,
            commands::reorder_watchlist,
            commands::set_display,
            commands::set_chart_settings,
            commands::upsert_alert,
            commands::delete_alert,
            commands::set_notifications,
            commands::send_test_notification,
            commands::set_autostart,
            commands::start_drag,
            commands::drag_ended,
            commands::set_pin,
            commands::toggle_expand,
            commands::collapse_if_unpinned,
            referral::commands::get_referral_partners,
            referral::commands::get_referral_profile,
            referral::commands::set_referral_partner,
            referral::commands::set_referral_id,
            referral::commands::set_referral_template,
            referral::commands::open_referral_url,
            futures::commands::get_futures_state,
            futures::commands::get_futures_keys,
            futures::commands::set_futures_keys,
            futures::commands::clear_futures_keys,
            futures::commands::set_futures_enabled,
            futures::commands::set_futures_venue,
            futures::commands::set_futures_preferences,
            futures::commands::refresh_futures,
            futures::commands::test_futures_connection,
            futures::commands::open_futures_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
