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

    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let pin_item = MenuItem::with_id(app, "pin", pin_label(pinned_now), true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings_item, &pin_item, &quit_item])?;

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
            "settings" => {
                // The renderer only mounts the settings pane inside the expanded layout, so a
                // bare `open-settings` emit while collapsed did nothing at all.
                commands::expand_internal(app);
                let _ = app.emit("open-settings", ());
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
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
                commands::toggle_expand_internal(app);
            }
        })
        .build(app)?;

    Ok(())
}
