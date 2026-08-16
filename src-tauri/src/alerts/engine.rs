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
        let fired = evaluate_pure(&mut settings.alerts, symbol, price, buffer, warmup_active, now);
        let notifications = settings.notifications.clone();
        drop(settings);

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

/// Pure decision logic: which of `alerts` should fire for this `symbol`/`price`, given the
/// current spike buffer and warmup state. Mutates matching alerts in place (`lastFiredAt`,
/// `enabled` for `once` rules) and returns clones of the ones that fired. Kept free of
/// `AppHandle` so it's unit-testable without a running Tauri app.
fn evaluate_pure(
    alerts: &mut [Alert],
    symbol: &str,
    price: f64,
    buffer: Option<&VecDeque<(Instant, f64)>>,
    warmup_active: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<Alert> {
    let mut fired: Vec<Alert> = Vec::new();

    for alert in alerts.iter_mut().filter(|a| a.enabled && a.symbol == symbol) {
        if let Some(last) = alert.last_fired_at {
            let elapsed = now.signed_duration_since(last);
            if elapsed < chrono::Duration::minutes(alert.cooldown_min as i64) {
                continue;
            }
        }

        let should_fire = match alert.kind {
            AlertKind::PriceAbove => price >= alert.value,
            AlertKind::PriceBelow => price <= alert.value,
            AlertKind::Spike => {
                if warmup_active {
                    false
                } else {
                    let window = Duration::from_secs(alert.window_minutes.unwrap_or(15) as u64 * 60);
                    buffer.and_then(|b| spike::detect(b, window, alert.value)).is_some()
                }
            }
        };

        if should_fire {
            alert.last_fired_at = Some(now);
            if alert.once {
                alert.enabled = false;
            }
            fired.push(alert.clone());
        }
    }

    fired
}

fn notify(app: &AppHandle, alert: &Alert, price: f64, notifications: &NotificationSettings) {
    // Both toggles were persisted but never read, so switching toasts off changed nothing and
    // the "Alert sound" switch in Settings was inert.
    if !notifications.toast {
        return;
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    #[test]
    fn fires_when_price_crosses_above() {
        let mut alerts = vec![price_above("a1", "BTCUSDT", 100.0)];
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 101.0, None, false, chrono::Utc::now());
        assert_eq!(fired.len(), 1);
        assert!(alerts[0].last_fired_at.is_some());
    }

    #[test]
    fn silent_when_price_below_threshold() {
        let mut alerts = vec![price_above("a1", "BTCUSDT", 100.0)];
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 99.0, None, false, chrono::Utc::now());
        assert!(fired.is_empty());
    }

    #[test]
    fn fires_when_price_crosses_below() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        alert.kind = AlertKind::PriceBelow;
        let mut alerts = vec![alert];
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 99.0, None, false, chrono::Utc::now());
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn cooldown_blocks_repeat_fire() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        let now = chrono::Utc::now();
        alert.last_fired_at = Some(now - chrono::Duration::minutes(5));
        alert.cooldown_min = 15;
        let mut alerts = vec![alert];
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 200.0, None, false, now);
        assert!(fired.is_empty(), "cooldown should suppress the repeat fire");
    }

    #[test]
    fn fires_again_after_cooldown_elapses() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        let now = chrono::Utc::now();
        alert.last_fired_at = Some(now - chrono::Duration::minutes(16));
        alert.cooldown_min = 15;
        let mut alerts = vec![alert];
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 200.0, None, false, now);
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn once_disables_after_firing() {
        let mut alert = price_above("a1", "BTCUSDT", 100.0);
        alert.once = true;
        let mut alerts = vec![alert];
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 200.0, None, false, chrono::Utc::now());
        assert_eq!(fired.len(), 1);
        assert!(!alerts[0].enabled, "once rule must disable itself after firing");
    }

    #[test]
    fn removed_symbol_does_not_panic() {
        let mut alerts: Vec<Alert> = vec![];
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 200.0, None, false, chrono::Utc::now());
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
        let fired = evaluate_pure(&mut alerts, "BTCUSDT", 110.0, Some(&buf), true, chrono::Utc::now());
        assert!(fired.is_empty(), "warmup must suppress spike alerts");
    }
}
