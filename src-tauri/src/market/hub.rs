use super::{ConnectionState, ConnectionStatus, TickerSnapshot};
use crate::alerts::engine::{AlertsEngine, PriceBuffers};
use crate::config::ConfigState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Central in-memory market state. Owned by Rust so live prices and alert evaluation keep
/// working while the panel is collapsed and the renderer isn't listening to anything.
pub struct MarketHub {
    app: AppHandle,
    config: Arc<ConfigState>,
    alerts: AlertsEngine,
    tickers: RwLock<HashMap<String, TickerSnapshot>>,
    dirty: AtomicBool,
    connection: RwLock<ConnectionStatus>,
    buffers: RwLock<PriceBuffers>,
    warmup_until: RwLock<Option<Instant>>,
}

impl MarketHub {
    pub fn new(app: AppHandle, config: Arc<ConfigState>) -> Arc<Self> {
        let alerts = AlertsEngine::new(config.clone());
        Arc::new(Self {
            app,
            config,
            alerts,
            tickers: RwLock::new(HashMap::new()),
            dirty: AtomicBool::new(false),
            connection: RwLock::new(ConnectionStatus {
                state: ConnectionState::Connecting,
                attempt: 0,
                latency_ms: None,
            }),
            buffers: RwLock::new(PriceBuffers::default()),
            warmup_until: RwLock::new(None),
        })
    }

    pub fn apply_ticker(&self, update: TickerSnapshot) {
        let warmup_active = self
            .warmup_until
            .read()
            .unwrap()
            .map(|until| Instant::now() < until)
            .unwrap_or(false);

        // A halted pair keeps returning the price of its last trade over REST forever. It is
        // still worth storing — the row shows it greyed out rather than as `···` — but it must
        // not reach the spike buffer or the alert engine, or a weeks-old number would keep
        // arming and firing rules as if the market had just printed it.
        let stale = update.is_stale(super::now_ms());

        // Two venues never agree to the last cent, so a source switch would otherwise land in
        // the buffer as an instant jump and read as a spike.
        let source_changed = self
            .tickers
            .read()
            .unwrap()
            .get(&update.symbol)
            .map(|prev| prev.source != update.source)
            .unwrap_or(false);
        if source_changed {
            self.buffers.write().unwrap().clear_symbol(&update.symbol);
        }

        if !stale {
            // `warmup_duration` is the longest enabled spike window; one extra minute keeps a
            // full window of history available at the moment the oldest point ages out.
            let retention = self.alerts.warmup_duration() + Duration::from_secs(60);
            {
                let mut buffers = self.buffers.write().unwrap();
                buffers.push(&update.symbol, update.price, retention);
            }

            self.alerts.evaluate(
                &self.app,
                &update.symbol,
                update.price,
                self.buffers.read().unwrap().get(&update.symbol),
                warmup_active,
            );
        }

        self.tickers
            .write()
            .unwrap()
            .insert(update.symbol.clone(), update);
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// True when the primary feed has nothing usable for `symbol` — no snapshot at all, or one
    /// that has aged past the staleness cutoff. Drives the backup-venue sweep.
    pub fn needs_backup(&self, symbol: &str) -> bool {
        let now = super::now_ms();
        self.tickers
            .read()
            .unwrap()
            .get(symbol)
            .map(|snapshot| snapshot.is_stale(now))
            .unwrap_or(true)
    }

    pub fn set_connection(&self, state: ConnectionState, attempt: u32) {
        let mut conn = self.connection.write().unwrap();
        conn.state = state;
        conn.attempt = attempt;
        drop(conn);
        self.emit_connection();
    }

    pub fn connection_status(&self) -> ConnectionStatus {
        self.connection.read().unwrap().clone()
    }

    /// Called whenever the WS connection is (re)established: clears stale rolling buffers
    /// and opens a warmup window so the gap can't be mistaken for a real spike.
    pub fn on_connected(&self) {
        let mut buffers = self.buffers.write().unwrap();
        buffers.clear();
        drop(buffers);
        *self.warmup_until.write().unwrap() = Some(Instant::now() + self.alerts.warmup_duration());
    }

    pub fn tickers_snapshot(&self) -> Vec<TickerSnapshot> {
        self.tickers.read().unwrap().values().cloned().collect()
    }

    pub fn config(&self) -> &Arc<ConfigState> {
        &self.config
    }

    fn emit_connection(&self) {
        let status = self.connection_status();
        let _ = self.app.emit("connection", &status);
    }

    /// Background task: emits the full ticker map to the frontend at most 4x/sec, coalescing
    /// bursty WS updates instead of re-rendering React on every single message.
    pub fn spawn_emit_loop(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));
            loop {
                interval.tick().await;
                if self
                    .dirty
                    .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    let snapshot = self.tickers_snapshot();
                    let _ = self.app.emit("tickers", &snapshot);
                }
            }
        });
    }
}
