//! Futures account state, owned by Rust and mirrored to the renderer.
//!
//! Same division of labour as `MarketHub`: the backend polls, holds the numbers, and emits;
//! React only renders what it is handed. The panel is collapsed most of the time, and a store
//! that only filled itself while a tab was visible would show a stale balance the moment it
//! reopened.
//!
//! Polling rather than streaming, for now: a read-only account view is well served by a signed
//! request every few seconds, and the WebSocket user-stream — with its listen-key keepalive and
//! its own reconnect ladder — is a second failure surface that buys latency this panel does not
//! need. The polling loop is the same fallback the market feed already drops to.

use super::venue::{FuturesSnapshot, FuturesVenue, NewOrder, OrderRecord, OrderReceipt, OrderSide};
use super::{FuturesSettings, VenueMode};
use crate::config::ConfigState;
use crate::secrets;
use serde::Serialize;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// How often the account is refreshed while the feature is on.
///
/// 6s at ~10 weight per poll is ~100 weight/min against a 2400/min allowance — unremarkable to
/// Binance, and fast enough that a position's PnL never looks frozen.
const POLL_INTERVAL: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FuturesStatus {
    /// The feature is switched off in settings.
    Off,
    /// On, but no API key is stored for the selected venue.
    NoKey,
    /// Key present, first successful response not in yet.
    Connecting,
    Live,
    /// The last poll failed. The previous numbers, if any, stay on screen.
    Error,
}

/// What the renderer receives on every `futures` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesState {
    pub status: FuturesStatus,
    pub mode: VenueMode,
    /// Populated for `Error`; also carries the "connected" summary after a manual test.
    pub message: Option<String>,
    /// The last snapshot that arrived, kept through an error so the panel never blanks —
    /// the same invariant the watchlist holds.
    pub snapshot: Option<FuturesSnapshot>,
    /// Whether orders may be placed at all. Mirrors [`VenueMode::allows_trading`], so the UI
    /// does not have to re-derive the rule the backend enforces.
    pub trading_allowed: bool,
}

impl FuturesState {
    fn new(status: FuturesStatus, mode: VenueMode) -> Self {
        Self {
            status,
            mode,
            message: None,
            snapshot: None,
            trading_allowed: mode.allows_trading(),
        }
    }
}

struct Inner {
    venue: Option<Arc<dyn FuturesVenue>>,
    state: FuturesState,
}

pub struct FuturesHub {
    app: AppHandle,
    config: Arc<ConfigState>,
    inner: RwLock<Inner>,
}

impl FuturesHub {
    pub fn new(app: AppHandle, config: Arc<ConfigState>) -> Arc<Self> {
        let settings = config.snapshot().futures;
        let hub = Arc::new(Self {
            app,
            config,
            inner: RwLock::new(Inner {
                venue: None,
                state: FuturesState::new(FuturesStatus::Off, settings.mode),
            }),
        });
        hub.reload();
        hub
    }

    pub fn state(&self) -> FuturesState {
        self.inner.read().unwrap().state.clone()
    }

    fn venue(&self) -> Option<Arc<dyn FuturesVenue>> {
        self.inner.read().unwrap().venue.clone()
    }

    /// Rebuilds the client from current settings and stored credentials.
    ///
    /// Called on startup and after anything that could change either — the venue switch, a key
    /// being saved or cleared, the feature being toggled. Cheap enough to be unconditional:
    /// it is a keyring read and a struct allocation, no network.
    pub fn reload(&self) {
        let settings: FuturesSettings = self.config.snapshot().futures;
        let mode = settings.mode;

        let (venue, status): (Option<Arc<dyn FuturesVenue>>, FuturesStatus) = if !settings.enabled {
            (None, FuturesStatus::Off)
        } else {
            match secrets::load(mode.keyring_namespace()) {
                Some(credential) => (
                    Some(Arc::new(super::binance::BinanceFutures::new(credential, mode))),
                    FuturesStatus::Connecting,
                ),
                None => (None, FuturesStatus::NoKey),
            }
        };

        {
            let mut inner = self.inner.write().unwrap();
            inner.venue = venue;
            // Switching venue must not carry mainnet positions into a testnet view.
            let keep_snapshot = inner.state.mode == mode && status == FuturesStatus::Connecting;
            let snapshot = if keep_snapshot { inner.state.snapshot.take() } else { None };
            inner.state = FuturesState {
                status,
                mode,
                message: None,
                snapshot,
                trading_allowed: mode.allows_trading(),
            };
        }
        self.emit();
    }

    /// Fetches once and records the outcome. Returns the error so a caller that asked for the
    /// refresh directly can report it, while the background loop can ignore it — the state has
    /// already been updated either way.
    pub async fn refresh(&self) -> Result<(), String> {
        let Some(venue) = self.venue() else {
            return Ok(());
        };

        // The lock is never held across this await: `venue` is an owned Arc.
        let result = venue.snapshot().await;

        {
            let mut inner = self.inner.write().unwrap();
            match &result {
                Ok(snapshot) => {
                    inner.state.status = FuturesStatus::Live;
                    inner.state.message = None;
                    inner.state.snapshot = Some(snapshot.clone());
                }
                Err(e) => {
                    inner.state.status = FuturesStatus::Error;
                    inner.state.message = Some(e.clone());
                    // `snapshot` is deliberately left as it was.
                }
            }
        }
        self.emit();
        result.map(|_| ())
    }

    /// Verifies the stored credentials against the venue, without disturbing the polled state.
    pub async fn check_credentials(&self) -> Result<String, String> {
        let venue = self
            .venue()
            .ok_or_else(|| "no API key stored for this venue".to_string())?;
        venue.check_credentials().await
    }

    /// The one gate every money-moving method below goes through. Reads the *current* state
    /// rather than the settings a caller might still be holding, so a venue switch mid-request
    /// cannot be raced into an order landing on mainnet.
    fn ensure_trading_allowed(&self) -> Result<(), String> {
        if self.inner.read().unwrap().state.trading_allowed {
            Ok(())
        } else {
            Err("orders can only be placed on the testnet".into())
        }
    }

    fn venue_or_err(&self) -> Result<std::sync::Arc<dyn FuturesVenue>, String> {
        self.venue().ok_or_else(|| "no API key stored for this venue".to_string())
    }

    /// Places an order, then folds a fresh snapshot into state so the position table reflects it
    /// on the very next render rather than waiting up to `POLL_INTERVAL` for the background loop.
    pub async fn place_order(&self, order: NewOrder) -> Result<OrderReceipt, String> {
        self.ensure_trading_allowed()?;
        let receipt = self.venue_or_err()?.place_order(order).await?;
        let _ = self.refresh().await;
        Ok(receipt)
    }

    pub async fn close_position(
        &self,
        symbol: String,
        side: OrderSide,
        quantity: Option<f64>,
    ) -> Result<OrderReceipt, String> {
        self.ensure_trading_allowed()?;
        let receipt = self
            .venue_or_err()?
            .close_position(&symbol, side, quantity)
            .await?;
        let _ = self.refresh().await;
        Ok(receipt)
    }

    pub async fn cancel_order(&self, symbol: String, order_id: i64) -> Result<(), String> {
        self.ensure_trading_allowed()?;
        self.venue_or_err()?.cancel_order(&symbol, order_id).await
    }

    pub async fn set_leverage(&self, symbol: String, leverage: u32) -> Result<(), String> {
        self.ensure_trading_allowed()?;
        self.venue_or_err()?.set_leverage(&symbol, leverage).await
    }

    pub async fn set_margin_type(&self, symbol: String, isolated: bool) -> Result<(), String> {
        self.ensure_trading_allowed()?;
        self.venue_or_err()?.set_margin_type(&symbol, isolated).await
    }

    /// Order history is a read, not a trade — available on mainnet too, unlike everything else
    /// in this block.
    pub async fn order_history(&self, symbol: String, limit: u32) -> Result<Vec<OrderRecord>, String> {
        self.venue_or_err()?.order_history(&symbol, limit).await
    }

    pub fn emit(&self) {
        let _ = self.app.emit("futures", self.state());
    }

    /// Background refresh. Runs for the life of the app and does nothing while the feature is
    /// off, so toggling it on does not need to start anything.
    pub fn spawn_poll_loop(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if self.venue().is_some() {
                    let _ = self.refresh().await;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_mirrors_the_trading_rule_rather_than_restating_it() {
        assert!(!FuturesState::new(FuturesStatus::Live, VenueMode::Mainnet).trading_allowed);
        assert!(FuturesState::new(FuturesStatus::Live, VenueMode::Testnet).trading_allowed);
    }

    #[test]
    fn the_state_serializes_for_the_renderer() {
        let json = serde_json::to_string(&FuturesState::new(FuturesStatus::NoKey, VenueMode::Testnet)).unwrap();
        assert!(json.contains("\"status\":\"nokey\""), "{json}");
        assert!(json.contains("\"tradingAllowed\":true"), "{json}");
        assert!(json.contains("\"snapshot\":null"), "{json}");
    }
}
