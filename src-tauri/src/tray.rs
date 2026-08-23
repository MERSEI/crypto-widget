use crate::commands;
use crate::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

fn pin_label(pinned: bool) -> &'static str {
    if pinned {
        "Unpin"
    } else {
        "Pin"
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let pinned_now = app
        .try_state::<AppState>()
        .map(|state| state.config.snapshot().window.pinned)
        .unwrap_or(false);

    let wallet_item = MenuItem::with_id(app, "wallet", "Wallet", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let pin_item = MenuItem::with_id(app, "pin", pin_label(pinned_now), true, None::<&str>)?;
    let test_item = MenuItem::with_id(app, "test-notification", "Test notification", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&wallet_item, &settings_item, &pin_item, &test_item, &quit_item])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("bundle icon missing");

    let pin_item_for_menu = pin_item.clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quit" => {
                // Autosave runs on a 500ms timer, so a setting changed right before Quit would
                // otherwise never reach disk.
                if let Some(state) = app.try_state::<AppState>() {
                    state.config.flush_if_dirty();
                }
                app.exit(0);
            }
            "wallet" => {
                // The wallet lives in its own window, so it is reachable even while the pill is
                // collapsed — which is most of the time.
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::wallet::commands::open_wallet_window(app).await {
                        eprintln!("wallet window: {e}");
                    }
                });
            }
            "settings" => {
                // The renderer only mounts the settings pane inside the expanded layout, so a
                // bare `open-settings` emit while collapsed did nothing at all.
                commands::expand_internal(app);
                let _ = app.emit("open-settings", ());
            }
            "test-notification" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = commands::fire_test_notification(app).await {
                        eprintln!("test notification: {e}");
                    }
                });
            }
            "pin" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let pinned = !state.config.snapshot().window.pinned;
                    commands::set_pinned_internal(app, pinned);
                    let _ = pin_item_for_menu.set_text(pin_label(pinned));
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                // With the pill switched off there is nothing for a left click to expand, and
                // showing it anyway would undo the setting. The wallet is what the icon means
                // then.
                let pill_enabled = app
                    .try_state::<AppState>()
                    .map(|state| state.config.snapshot().wallet.widget_enabled)
                    .unwrap_or(true);
                if !pill_enabled {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::wallet::commands::open_wallet_window(app).await {
                            eprintln!("wallet window: {e}");
                        }
                    });
                    return;
                }
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
                commands::toggle_expand_internal(app);
            }
        })
        .build(app)?;

    Ok(())
}
