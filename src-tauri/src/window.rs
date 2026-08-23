use crate::config::{Edge, WindowSettings};
use tauri::{LogicalPosition, LogicalSize, WebviewWindow};

/// A horizontal bar rather than a vertical strip: rotated text in a 28px-wide sliver was
/// unreadable at a glance, which is the one thing this widget exists to do.
pub const PILL_WIDTH: f64 = 140.0;
pub const PILL_HEIGHT: f64 = 30.0;

pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Approximates the OS work area with the monitor's full bounds (no taskbar exclusion).
/// Documented MVP simplification: true per-platform work area needs Win32 GDI FFI.
/// Risk accepted per ТЗ раздел 15 (DPI/monitor edge cases, "medium probability, acceptable").
pub fn work_area(window: &WebviewWindow) -> Option<Rect> {
    let monitor = window.current_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let pos = monitor.position().to_logical::<f64>(scale);
    Some(Rect {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    })
}

pub fn geometry_for(edge: Edge, offset: f64, area: &Rect, size: (f64, f64)) -> (f64, f64, f64, f64) {
    let (w, h) = size;
    let offset = offset.clamp(0.0, 1.0);
    match edge {
        Edge::Right => (
            area.x + area.width - w,
            area.y + (area.height - h) * offset,
            w,
            h,
        ),
        Edge::Left => (area.x, area.y + (area.height - h) * offset, w, h),
        Edge::Top => (area.x + (area.width - w) * offset, area.y, w, h),
        Edge::Bottom => (
            area.x + (area.width - w) * offset,
            area.y + area.height - h,
            w,
            h,
        ),
    }
}

fn geometry_target(window: &WebviewWindow, settings: &WindowSettings, size: (f64, f64)) -> Option<(f64, f64, f64, f64)> {
    let area = work_area(window)?;
    Some(geometry_for(settings.edge, settings.offset, &area, size))
}

/// Unconditionally sets the window's size and position. Used for every geometry change whose
/// caller already knows it wants a real resize — expand/collapse and the one-shot snap at the
/// end of a drag — as opposed to [`apply_geometry_if_unsettled`], whose skip-if-already-there
/// guard exists for a narrower, self-triggering case (see its doc comment). Skipping here on a
/// stale `outer_size()`/`outer_position()` reading silently dropped the resize entirely — the
/// window stayed pill-sized while the renderer had already painted the full panel, fixable only
/// by a restart, because nothing else would ever call this again to retry it.
fn apply_geometry(window: &WebviewWindow, settings: &WindowSettings, size: (f64, f64)) {
    let Some((x, y, w, h)) = geometry_target(window, settings, size) else {
        return;
    };
    let _ = window.set_size(LogicalSize::new(w, h));
    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// Docks the window to `settings.edge`, but only when it isn't already there. The no-op guard
/// matters here specifically because this path is driven by `WindowEvent::Moved`: repositioning
/// the window emits another `Moved`, so an unconditional apply would keep re-triggering itself
/// forever. Only `snap_to_edge` (the post-drag re-dock) should use this — anything that isn't
/// itself reacting to a `Moved` event should call [`apply_geometry`] instead.
fn apply_geometry_if_unsettled(window: &WebviewWindow, settings: &WindowSettings, size: (f64, f64)) {
    let Some((x, y, w, h)) = geometry_target(window, settings, size) else {
        return;
    };

    if let (Ok(pos), Ok(scale), Ok(current)) = (
        window.outer_position(),
        window.scale_factor(),
        window.outer_size(),
    ) {
        let pos = pos.to_logical::<f64>(scale);
        let current = current.to_logical::<f64>(scale);
        let settled = (pos.x - x).abs() < 1.0
            && (pos.y - y).abs() < 1.0
            && (current.width - w).abs() < 1.0
            && (current.height - h).abs() < 1.0;
        if settled {
            return;
        }
    }

    let _ = window.set_size(LogicalSize::new(w, h));
    let _ = window.set_position(LogicalPosition::new(x, y));
}

pub fn apply_pill_geometry(window: &WebviewWindow, settings: &WindowSettings) {
    apply_geometry(window, settings, (PILL_WIDTH, PILL_HEIGHT));
}

pub fn apply_panel_geometry(window: &WebviewWindow, settings: &WindowSettings) {
    apply_geometry(
        window,
        settings,
        (settings.panel_width, settings.panel_height),
    );
}

/// Re-docks to `settings.edge` unless the window is already there — see
/// [`apply_geometry_if_unsettled`]. Only for use from the `Moved`-driven edge-snap.
pub fn snap_pill_geometry(window: &WebviewWindow, settings: &WindowSettings) {
    apply_geometry_if_unsettled(window, settings, (PILL_WIDTH, PILL_HEIGHT));
}

pub fn snap_panel_geometry(window: &WebviewWindow, settings: &WindowSettings) {
    apply_geometry_if_unsettled(
        window,
        settings,
        (settings.panel_width, settings.panel_height),
    );
}

/// After a manual drag, recompute {edge, offset} from wherever the window ended up so the
/// stored position is a resolution-independent fraction, not raw pixels. `size` is the
/// window's current logical size — the pill and the expanded panel have different footprints.
pub fn edge_offset_from_position(
    window: &WebviewWindow,
    x: f64,
    y: f64,
    size: (f64, f64),
) -> Option<(Edge, f64)> {
    let (w, h) = size;
    let area = work_area(window)?;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let dist_left = cx - area.x;
    let dist_right = (area.x + area.width) - cx;
    let dist_top = cy - area.y;
    let dist_bottom = (area.y + area.height) - cy;
    let min = dist_left.min(dist_right).min(dist_top).min(dist_bottom);

    let edge = if min == dist_left {
        Edge::Left
    } else if min == dist_right {
        Edge::Right
    } else if min == dist_top {
        Edge::Top
    } else {
        Edge::Bottom
    };

    let offset = match edge {
        Edge::Left | Edge::Right => ((y - area.y) / (area.height - h).max(1.0)).clamp(0.0, 1.0),
        Edge::Top | Edge::Bottom => ((x - area.x) / (area.width - w).max(1.0)).clamp(0.0, 1.0),
    };

    Some((edge, offset))
}
