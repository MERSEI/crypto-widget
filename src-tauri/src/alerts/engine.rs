use super::spike;
use crate::config::{Alert, AlertKind, ConfigState, NotificationSettings};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

pub struct AlertsEngine {
    config: Arc<ConfigState>,
}

impl AlertsEngine {
    pub fn new(config: Arc<ConfigState>) -> Self {
        Self { config }
    }

    /// The warmup period after (re)connect: the longest window any enabled spike rule cares
    /// about, so a stale->fresh price jump across the gap can never look like a real spike.
    pub fn warmup_duration(&self) -> Duration {
        let settings = self.config.settings.read().unwrap();
        let minutes = settings
            .alerts
            .iter()
            .filter(|a| a.enabled && a.kind == AlertKind::Spike)
            .filter_map(|a| a.window_minutes)
            .max()
            .unwrap_or(15);
        Duration::from_secs(minutes as u64 * 60)
    }

    pub fn evaluate(
        &self,
        app: &AppHandle,
        symbol: &str,
        price: f64,
        buffer: Option<&VecDeque<(Instant, f64)>>,
        warmup_active: bool,
    ) {
        let mut settings = self.config.settings.write().unwrap();
        let now = chrono::Utc::now();
        let Evaluation { fired, armed_changed } =
            evaluate_pure(&mut settings.alerts, symbol, price, buffer, warmup_active, now);
        let notifications = settings.notifications.clone();
        drop(settings);

        // Crossing state moves without anything firing — a rule that just watched the price
        // come back below its level has to survive a restart in that state.
        if armed_changed {
            self.config.mark_dirty();
        }

        if !fired.is_empty() {
            self.config.mark_dirty();
            // Firing mutates the rules themselves (`lastFiredAt`, and `enabled` for `once`), so
            // without this the panel keeps showing a spent one-shot alert as active.
            let _ = app.emit("alerts", &self.config.snapshot().alerts);
            for alert in fired {
                notify(app, &alert, price, &notifications);
            }
        }
    }
}

pub struct Evaluation {
    /// Clones of the rules that fired on this price.
    pub fired: Vec<Alert>,
    /// Whether any rule's crossing state moved, firing or not — the caller persists on it.
    pub armed_changed: bool,
}

/// Pure decision logic: which of `alerts` should fire for this `symbol`/`price`, given the
/// current spike buffer and warmup state. Mutates matching alerts in place (`lastFiredAt`,
/// `armed`, and `enabled` for `once` rules) and returns clones of the ones that fired. Kept
/// free of `AppHandle` so it's unit-testable without a running Tauri app.
fn evaluate_pure(
    alerts: &mut [Alert],
    symbol: &str,
    price: f64,
    buffer: Option<&VecDeque<(Instant, f64)>>,
    warmup_active: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> Evaluation {
    let mut fired: Vec<Alert> = Vec::new();
    let mut armed_changed = false;

    for alert in alerts.iter_mut().filter(|a| a.enabled && a.symbol == symbol) {
        // Checked after the crossing state below is updated, not before: a level taken while
        // the rule was cooling down is still a level taken, and pretending the tick never
        // happened would leave the rule armed and fire it on the next tick past the level.
        let cooling_down = alert
            .last_fired_at
            .map(|last| now.signed_duration_since(last) < chrono::Duration::minutes(alert.cooldown_min as i64))
            .unwrap_or(false);

        let armed_before = alert.armed;
        let should_fire = match alert.kind {
            AlertKind::PriceAbove | AlertKind::PriceBelow => {
                let beyond = match alert.kind {
                    AlertKind::PriceAbove => price >= alert.value,
                    _ => price <= alert.value,
                };
                match alert.armed {
                    // First price this rule has ever seen. Sitting beyond the level is not a
                    // crossing — that is exactly how a frozen TONUSDT at 1.60 fired a
                    // "crossed above 1.3" toast — so the rule only records where it started.
                    None => {
                        alert.armed = Some(!beyond);
                        false
                    }
                    Some(true) if beyond => {
                        alert.armed = Some(false);
                        true
                    }
                    // Back on the waiting side: the level can count again.
                    Some(false) if !beyond => {
                        alert.armed = Some(true);
                        false
                    }
                    _ => false,
                }
            }
            AlertKind::Spike => {
                if warmup_active {
                    false
                } else {
                    let window = Duration::from_secs(alert.window_minutes.unwrap_or(15) as u64 * 60);
                    buffer.and_then(|b| spike::detect(b, window, alert.value)).is_some()
                }
            }
        };

        armed_changed |= alert.armed != armed_before;

        if should_fire && !cooling_down {
            alert.last_fired_at = Some(now);
            if alert.once {
                alert.enabled = false;
            }
            fired.push(alert.clone());
        }
    }

    Evaluation { fired, armed_changed }
}

/// Builds a rule that is never stored in settings — it only exists to carry the text of a
/// "what would a spike alert look like" toast through `notify`, so the test path and the real
/// path can't drift apart.
pub fn test_spike_alert(symbol: &str, percent: f64, window_minutes: u32) -> Alert {
    Alert {
        id: "test".into(),
        symbol: symbol.into(),
        kind: AlertKind::Spike,
        value: percent,
        window_minutes: Some(window_minutes),
        cooldown_min: 0,
        once: false,
        enabled: true,
        last_fired_at: None,
        armed: None,
    }
}

/// Returns false when the toast was suppressed by the settings toggle — the caller surfaces
/// that instead of leaving a "test" click looking like a broken notification stack.
pub fn notify(
    app: &AppHandle,
    alert: &Alert,
    price: f64,
    notifications: &NotificationSettings,
) -> bool {
    // Both toggles were persisted but never read, so switching toasts off changed nothing and
    // the "Alert sound" switch in Settings was inert.
    if !notifications.toast {
        return false;
    }
    let body = match alert.kind {
        AlertKind::PriceAbove => format!("{} crossed above {}", alert.symbol, alert.value),
        AlertKind::PriceBelow => format!("{} crossed below {}", alert.symbol, alert.value),
        AlertKind::Spike => format!(
            "{} moved {:.1}%+ in {} min (now {:.4})",
            alert.symbol,
            alert.value,
            alert.window_minutes.unwrap_or(15),
            price
        ),
    };
    let mut builder = app.notification().builder().title("crypto-widget alert").body(&body);
    if notifications.sound {
        // Windows resolves "Default" to the standard notification chime; a silent toast is what
        // you get without it.
        builder = builder.sound("Default");
    }
    let _ = builder.show();
    true
}

#[derive(Default, Clone)]
pub struct PriceBuffers {
    inner: HashMap<String, VecDeque<(Instant, f64)>>,
}

impl PriceBuffers {
    /// `retention` should cover the longest spike window in use and little more: Binance pushes
    /// roughly one tick per second per symbol, so the old fixed 24h retention parked ~86k points
    /// per coin in RAM to serve a detector that never looks further back than its own window.
    pub fn push(&mut self, symbol: &str, price: f64, retention: Duration) {
        let buf = self.inner.entry(symbol.to_string()).or_default();
        buf.push_back((Instant::now(), price));
        spike::trim(buf, retention);
    }

    pub fn get(&self, symbol: &str) -> Option<&VecDeque<(Instant, f64)>> {
        self.inner.get(symbol)
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Drops the history of a single symbol, used when its price starts coming from a
    /// different venue.
    pub fn clear_symbol(&mut self, symbol: &str) {
        self.inner.remove(symbol);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test shim: the arming bookkeeping is asserted through `alerts[..].armed` directly, so
    /// the cases below only care about which rules fired.
    fn fired_by(
        alerts: &mut [Alert],
        symbol: &str,
        price: f64,
        buffer: Option<&VecDeque<(Instant, f64)>>,
        warmup_active: bool,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Vec<Alert> {
        evaluate_pure(alerts, symbol, price, buffer, warmup_active, now).fired
    }

    /// An armed rule: the price is on the waiting side of the level, so the next tick past it
    /// is a genuine crossing.
    fn price_above(id: &str, symbol: &str, value: f64) -> Alert {
        Alert {
            id: id.into(),
            symbol: symbol.into(),
            kind: AlertKind::PriceAbove,
            value,
            window_minutes: None,
            cooldown_min: 15,
            once: false,
            enabled: true,
            last_fired_at: None,
            armed: Some(true),
        }
    }

    #[test]
    fn fires_when_price_crosses_above() {
        let mut alerts = vec![price_above("a1", "BTCUSDT", 100.0)];
        let fired = fired_by(&mut alerts, "BTCUSDT", 101.0, None, false, chrono::Utc::now());
        assert_eq!(fired.len(), 1);
        assert!(alerts[0].last_fired_at.is_some());
    }

    #[test]
    fn silent_when_price_below_threshold() {
        let mut alerts = vec![price_above("a1", "BTCUSDT", 100.0)];
        let fired = fired_by(&mut alerts, "BTCUSDT", 99.0, None, false, chrono::Utc::now());
        assert!(fired.is_empty());
    }

    #[test]
    fn fires_when_price_crosses_below() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        alert.kind = AlertKind::PriceBelow;
        let mut alerts = vec![alert];
        let fired = fired_by(&mut alerts, "BTCUSDT", 99.0, None, false, chrono::Utc::now());
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn a_level_already_taken_on_the_first_tick_only_arms() {
        // The TONUSDT case: a rule created while the (frozen) price already sat above the
        // level used to fire immediately.
        let mut alert = price_above("a1", "TONUSDT", 1.3);
        alert.armed = None;
        let mut alerts = vec![alert];
        let fired = fired_by(&mut alerts, "TONUSDT", 1.6, None, false, chrono::Utc::now());
        assert!(fired.is_empty(), "sitting beyond the level is not a crossing");
        assert_eq!(alerts[0].armed, Some(false));
    }

    #[test]
    fn holding_above_the_level_does_not_fire_twice() {
        let mut alerts = vec![price_above("a1", "BTCUSDT", 100.0)];
        let now = chrono::Utc::now();
        assert_eq!(fired_by(&mut alerts, "BTCUSDT", 101.0, None, false, now).len(), 1);
        alerts[0].cooldown_min = 0;
        assert!(fired_by(&mut alerts, "BTCUSDT", 105.0, None, false, now).is_empty());
        assert!(fired_by(&mut alerts, "BTCUSDT", 110.0, None, false, now).is_empty());
    }

    #[test]
    fn refires_after_dipping_back_below_the_level() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        alert.cooldown_min = 0;
        let mut alerts = vec![alert];
        let now = chrono::Utc::now();
        assert_eq!(fired_by(&mut alerts, "BTCUSDT", 101.0, None, false, now).len(), 1);
        assert!(fired_by(&mut alerts, "BTCUSDT", 98.0, None, false, now).is_empty());
        assert_eq!(alerts[0].armed, Some(true), "dipping back must re-arm the rule");
        assert_eq!(fired_by(&mut alerts, "BTCUSDT", 101.0, None, false, now).len(), 1);
    }

    #[test]
    fn a_crossing_during_cooldown_is_consumed_not_deferred() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        let now = chrono::Utc::now();
        alert.last_fired_at = Some(now - chrono::Duration::minutes(1));
        let mut alerts = vec![alert];
        assert!(fired_by(&mut alerts, "BTCUSDT", 101.0, None, false, now).is_empty());
        assert_eq!(
            alerts[0].armed,
            Some(false),
            "the level was taken; the rule must wait for a fresh crossing, not fire the moment cooldown ends"
        );
    }

    #[test]
    fn cooldown_blocks_repeat_fire() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        let now = chrono::Utc::now();
        alert.last_fired_at = Some(now - chrono::Duration::minutes(5));
        alert.cooldown_min = 15;
        let mut alerts = vec![alert];
        let fired = fired_by(&mut alerts, "BTCUSDT", 200.0, None, false, now);
        assert!(fired.is_empty(), "cooldown should suppress the repeat fire");
    }

    #[test]
    fn fires_again_after_cooldown_elapses() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        let now = chrono::Utc::now();
        alert.last_fired_at = Some(now - chrono::Duration::minutes(16));
        alert.cooldown_min = 15;
        let mut alerts = vec![alert];
        let fired = fired_by(&mut alerts, "BTCUSDT", 200.0, None, false, now);
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn once_disables_after_firing() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        alert.once = true;
        let mut alerts = vec![alert];
        let fired = fired_by(&mut alerts, "BTCUSDT", 200.0, None, false, chrono::Utc::now());
        assert_eq!(fired.len(), 1);
        assert!(!alerts[0].enabled, "once rule must disable itself after firing");
    }

    #[test]
    fn removed_symbol_does_not_panic() {
        let mut alerts: Vec<Alert> = vec![];
        let fired = fired_by(&mut alerts, "BTCUSDT", 200.0, None, false, chrono::Utc::now());
        assert!(fired.is_empty());
    }

    #[test]
    fn buffer_keeps_only_the_retention_window() {
        let mut buffers = PriceBuffers::default();
        for _ in 0..5 {
            buffers.push("BTCUSDT", 100.0, Duration::from_secs(3600));
        }
        assert_eq!(buffers.get("BTCUSDT").unwrap().len(), 5);

        // A zero retention drops everything already in the buffer. Whether the point pushed on
        // this very call survives depends on whether the clock ticked between its timestamp and
        // `trim`'s, so assert the bound rather than the exact count — age-based trimming itself
        // is pinned down deterministically by `spike::tests::trim_drops_old_points_only`.
        buffers.push("BTCUSDT", 101.0, Duration::ZERO);
        assert!(buffers.get("BTCUSDT").unwrap().len() <= 1);
    }

    #[test]
    fn spike_stays_silent_during_warmup() {
        let mut alert = price_above("a1", "BTCUSDT", 3.0);
        alert.kind = AlertKind::Spike;
        alert.window_minutes = Some(15);
        let mut alerts = vec![alert];
        let mut buf: VecDeque<(Instant, f64)> = VecDeque::new();
        buf.push_back((Instant::now(), 100.0));
        buf.push_back((Instant::now(), 110.0));
        let fired = fired_by(&mut alerts, "BTCUSDT", 110.0, Some(&buf), true, chrono::Utc::now());
        assert!(fired.is_empty(), "warmup must suppress spike alerts");
    }
}
