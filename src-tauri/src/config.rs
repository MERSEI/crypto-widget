use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use tauri::{AppHandle, Manager};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Right,
    Left,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSettings {
    pub edge: Edge,
    pub offset: f64,
    pub panel_width: f64,
    pub panel_height: f64,
    pub pinned: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            edge: Edge::Right,
            offset: 0.5,
            panel_width: 380.0,
            panel_height: 520.0,
            pinned: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub quote: String,
    pub fiat: Option<String>,
    pub theme: String,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            quote: "USDT".into(),
            fiat: None,
            theme: "terminal-dark".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSettings {
    pub default_timeframe: String,
    #[serde(rename = "type")]
    pub kind: String,
}

impl Default for ChartSettings {
    fn default() -> Self {
        Self {
            default_timeframe: "4h".into(),
            kind: "area".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistItem {
    pub symbol: String,
    pub order: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    PriceAbove,
    PriceBelow,
    Spike,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub symbol: String,
    pub kind: AlertKind,
    pub value: f64,
    #[serde(default)]
    pub window_minutes: Option<u32>,
    pub cooldown_min: u32,
    pub once: bool,
    pub enabled: bool,
    #[serde(default)]
    pub last_fired_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Crossing state for `price_above` / `price_below`: `Some(true)` means the price is on the
    /// waiting side of the level and a crossing would fire, `Some(false)` means the level has
    /// already been taken and the price has to come back before it counts again. `None` is a
    /// rule that has not seen a price yet — the first tick only arms it, so a level the market
    /// is already beyond never fires on the spot.
    #[serde(default)]
    pub armed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub toast: bool,
    pub sound: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            toast: true,
            sound: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub version: u32,
    pub window: WindowSettings,
    pub display: DisplaySettings,
    pub chart: ChartSettings,
    pub watchlist: Vec<WatchlistItem>,
    pub alerts: Vec<Alert>,
    pub notifications: NotificationSettings,
    pub autostart: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            window: WindowSettings::default(),
            display: DisplaySettings::default(),
            chart: ChartSettings::default(),
            watchlist: Vec::new(),
            alerts: Vec::new(),
            notifications: NotificationSettings::default(),
            autostart: false,
        }
    }
}

pub struct ConfigState {
    pub settings: RwLock<AppSettings>,
    dirty: AtomicBool,
    settings_path: PathBuf,
    pub cache_dir: PathBuf,
}

impl ConfigState {
    pub fn load(app: &AppHandle) -> Self {
        let config_dir = app
            .path()
            .app_config_dir()
            .expect("app_config_dir unavailable");
        let _ = fs::create_dir_all(&config_dir);
        let cache_dir = config_dir.join("cache");
        let _ = fs::create_dir_all(&cache_dir);

        let settings_path = config_dir.join("settings.json");
        let settings = load_settings_from(&config_dir);

        Self {
            settings: RwLock::new(settings),
            dirty: AtomicBool::new(false),
            settings_path,
            cache_dir,
        }
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    pub fn flush_if_dirty(&self) {
        if self
            .dirty
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let snapshot = self.settings.read().unwrap().clone();
            if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
                let tmp_path = self.settings_path.with_extension("json.tmp");
                if fs::write(&tmp_path, json).is_ok() {
                    let _ = fs::rename(&tmp_path, &self.settings_path);
                }
            }
        }
    }

    pub fn snapshot(&self) -> AppSettings {
        self.settings.read().unwrap().clone()
    }
}

/// Reads `settings.json` from `config_dir`, falling back to defaults on a missing file and
/// backing up + falling back to defaults on an unparsable one — never panics either way.
fn load_settings_from(config_dir: &std::path::Path) -> AppSettings {
    let settings_path = config_dir.join("settings.json");
    match fs::read_to_string(&settings_path) {
        Ok(raw) => match serde_json::from_str::<AppSettings>(&raw) {
            Ok(parsed) => parsed,
            Err(_) => {
                let corrupt_path = config_dir.join("settings.corrupt.json");
                let _ = fs::write(&corrupt_path, &raw);
                AppSettings::default()
            }
        },
        Err(_) => AppSettings::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let dir = std::env::temp_dir().join(format!("crypto-widget-config-test-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_falls_back_to_defaults() {
        let dir = temp_dir();
        let settings = load_settings_from(&dir);
        assert_eq!(settings.version, SETTINGS_VERSION);
        assert!(settings.watchlist.is_empty());
    }

    #[test]
    fn round_trips_through_json() {
        let dir = temp_dir();
        let mut settings = AppSettings::default();
        settings.watchlist.push(WatchlistItem {
            symbol: "BTCUSDT".into(),
            order: 0,
        });
        settings.window.edge = Edge::Left;
        settings.window.offset = 0.25;

        let path = dir.join("settings.json");
        fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();

        let loaded = load_settings_from(&dir);
        assert_eq!(loaded.watchlist.len(), 1);
        assert_eq!(loaded.watchlist[0].symbol, "BTCUSDT");
        assert_eq!(loaded.window.edge, Edge::Left);
        assert!((loaded.window.offset - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn corrupt_json_backs_up_and_falls_back_without_panicking() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        fs::write(&path, "{ not valid json").unwrap();

        let loaded = load_settings_from(&dir);
        assert_eq!(loaded.version, SETTINGS_VERSION);

        let corrupt_path = dir.join("settings.corrupt.json");
        assert!(corrupt_path.exists(), "corrupt file should have been backed up");
        let backed_up = fs::read_to_string(&corrupt_path).unwrap();
        assert_eq!(backed_up, "{ not valid json");
    }
}

/// Background task: flushes dirty settings to disk at most twice a second.
pub fn spawn_autosave(state: std::sync::Arc<ConfigState>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
        loop {
            interval.tick().await;
            state.flush_if_dirty();
        }
    });
}

/// Generic on-disk cache with a TTL, used for exchange_info / fx / last_tickers.
pub mod cache {
    use serde::{de::DeserializeOwned, Serialize};
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    #[derive(Serialize, serde::Deserialize)]
    struct Envelope<T> {
        saved_at: u64,
        value: T,
    }

    pub fn read_if_fresh<T: DeserializeOwned>(path: &Path, ttl: Duration) -> Option<T> {
        let raw = fs::read_to_string(path).ok()?;
        let envelope: Envelope<T> = serde_json::from_str(&raw).ok()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();
        if now.saturating_sub(envelope.saved_at) <= ttl.as_secs() {
            Some(envelope.value)
        } else {
            None
        }
    }

    /// Reads the cached value regardless of freshness (used as a last-resort fallback).
    pub fn read_stale<T: DeserializeOwned>(path: &Path) -> Option<T> {
        let raw = fs::read_to_string(path).ok()?;
        let envelope: Envelope<T> = serde_json::from_str(&raw).ok()?;
        Some(envelope.value)
    }

    pub fn write<T: Serialize>(path: &Path, value: &T) {
        let saved_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let envelope = Envelope {
            saved_at,
            value,
        };
        if let Ok(json) = serde_json::to_string(&envelope) {
            let _ = fs::write(path, json);
        }
    }
}
