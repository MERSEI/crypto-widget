use super::hub::MarketHub;
use super::provider::MarketProvider;
use super::{ConnectionState, TickerSnapshot};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const WS_BASE: &str = "wss://stream.binance.com:9443/stream";
const STALE_TIMEOUT: Duration = Duration::from_secs(10);
const PLANNED_RECONNECT_AFTER: Duration = Duration::from_secs(23 * 3600);
const FALLBACK_AFTER_FAILURES: u32 = 3;
const FALLBACK_POLL_INTERVAL: Duration = Duration::from_secs(10);
const WS_RETRY_WHILE_POLLING: Duration = Duration::from_secs(60);

pub enum WsCommand {
    SetSymbols(Vec<String>),
}

#[derive(Clone)]
pub struct WsHandle {
    tx: mpsc::UnboundedSender<WsCommand>,
}

impl WsHandle {
    pub fn set_symbols(&self, symbols: Vec<String>) {
        let _ = self.tx.send(WsCommand::SetSymbols(symbols));
    }
}

pub fn spawn(hub: Arc<MarketHub>, provider: Arc<dyn MarketProvider>) -> WsHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    tauri::async_runtime::spawn(run(hub, provider, rx));
    WsHandle { tx }
}

struct Backoff {
    attempt: u32,
}

impl Backoff {
    fn new() -> Self {
        Self { attempt: 0 }
    }

    fn reset(&mut self) {
        self.attempt = 0;
    }

    fn next_delay(&mut self) -> Duration {
        const STEPS_SECS: [u64; 6] = [1, 2, 4, 8, 16, 30];
        let idx = (self.attempt as usize).min(STEPS_SECS.len() - 1);
        self.attempt += 1;
        let jitter = rand::thread_rng().gen_range(-0.2..0.2_f64);
        Duration::from_secs_f64(STEPS_SECS[idx] as f64 * (1.0 + jitter))
    }
}

enum Outcome {
    PlannedReconnect,
    SymbolsEmptied,
    Stale,
    Error,
}

async fn run(
    hub: Arc<MarketHub>,
    provider: Arc<dyn MarketProvider>,
    mut cmd_rx: mpsc::UnboundedReceiver<WsCommand>,
) {
    let mut symbols: Vec<String> = Vec::new();
    let mut backoff = Backoff::new();
    let mut consecutive_failures: u32 = 0;

    loop {
        while symbols.is_empty() {
            hub.set_connection(ConnectionState::Offline, 0);
            match cmd_rx.recv().await {
                Some(WsCommand::SetSymbols(s)) => symbols = s,
                None => return,
            }
        }

        hub.set_connection(ConnectionState::Connecting, backoff.attempt);
        let outcome = run_connection(&hub, &mut symbols, &mut cmd_rx).await;

        match outcome {
            Outcome::PlannedReconnect => {
                backoff.reset();
                consecutive_failures = 0;
                continue;
            }
            Outcome::SymbolsEmptied => {
                continue;
            }
            Outcome::Stale | Outcome::Error => {
                consecutive_failures += 1;
            }
        }

        if consecutive_failures >= FALLBACK_AFTER_FAILURES {
            hub.set_connection(ConnectionState::Polling, backoff.attempt);
            run_fallback_polling(&hub, &provider, &mut symbols, &mut cmd_rx).await;
            consecutive_failures = 0;
            backoff.reset();
            continue;
        }

        hub.set_connection(ConnectionState::Reconnecting, backoff.attempt);
        let delay = backoff.next_delay();
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(WsCommand::SetSymbols(s)) => symbols = s,
                    None => return,
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct CombinedEnvelope {
    data: TickerFrame,
}

#[derive(Deserialize)]
struct TickerFrame {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "c")]
    last_price: String,
    #[serde(rename = "P")]
    percent_24h: String,
    #[serde(rename = "q")]
    quote_volume: String,
    #[serde(rename = "E")]
    event_time: i64,
}

async fn run_connection(
    hub: &Arc<MarketHub>,
    symbols: &mut Vec<String>,
    cmd_rx: &mut mpsc::UnboundedReceiver<WsCommand>,
) -> Outcome {
    let streams = symbols
        .iter()
        .map(|s| format!("{}@ticker", s.to_lowercase()))
        .collect::<Vec<_>>()
        .join("/");
    let url = format!("{WS_BASE}?streams={streams}");

    let mut ws = match tokio_tungstenite::connect_async(&url).await {
        Ok((stream, _resp)) => stream,
        Err(e) => {
            eprintln!("ws connect failed: {e}");
            return Outcome::Error;
        }
    };

    hub.on_connected();
    hub.set_connection(ConnectionState::Live, 0);

    let connected_at = Instant::now();
    let mut last_recv = Instant::now();
    let mut check_tick = tokio::time::interval(Duration::from_secs(2));
    let mut control_id: u64 = 1;

    loop {
        tokio::select! {
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_recv = Instant::now();
                        if let Ok(env) = serde_json::from_str::<CombinedEnvelope>(&text) {
                            hub.apply_ticker(TickerSnapshot {
                                symbol: env.data.symbol,
                                price: env.data.last_price.parse().unwrap_or(0.0),
                                percent_24h: env.data.percent_24h.parse().unwrap_or(0.0),
                                quote_volume: env.data.quote_volume.parse().unwrap_or(0.0),
                                event_time: env.data.event_time,
                            });
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        last_recv = Instant::now();
                        if ws.send(Message::Pong(payload)).await.is_err() {
                            return Outcome::Error;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_recv = Instant::now();
                    }
                    Some(Ok(Message::Close(_))) => return Outcome::Error,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        eprintln!("ws read error: {e}");
                        return Outcome::Error;
                    }
                    None => return Outcome::Error,
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(WsCommand::SetSymbols(new_symbols)) => {
                        if new_symbols.is_empty() {
                            *symbols = new_symbols;
                            return Outcome::SymbolsEmptied;
                        }
                        let added: Vec<String> = new_symbols
                            .iter()
                            .filter(|s| !symbols.contains(s))
                            .cloned()
                            .collect();
                        let removed: Vec<String> = symbols
                            .iter()
                            .filter(|s| !new_symbols.contains(s))
                            .cloned()
                            .collect();
                        if !added.is_empty() {
                            control_id += 1;
                            let params: Vec<String> =
                                added.iter().map(|s| format!("{}@ticker", s.to_lowercase())).collect();
                            let frame = serde_json::json!({"method":"SUBSCRIBE","params":params,"id":control_id});
                            let _ = ws.send(Message::Text(frame.to_string())).await;
                        }
                        if !removed.is_empty() {
                            control_id += 1;
                            let params: Vec<String> =
                                removed.iter().map(|s| format!("{}@ticker", s.to_lowercase())).collect();
                            let frame = serde_json::json!({"method":"UNSUBSCRIBE","params":params,"id":control_id});
                            let _ = ws.send(Message::Text(frame.to_string())).await;
                        }
                        *symbols = new_symbols;
                    }
                    None => return Outcome::Error,
                }
            }
            _ = check_tick.tick() => {
                if last_recv.elapsed() > STALE_TIMEOUT {
                    hub.set_connection(ConnectionState::Stale, 0);
                    return Outcome::Stale;
                }
                if connected_at.elapsed() > PLANNED_RECONNECT_AFTER {
                    return Outcome::PlannedReconnect;
                }
            }
        }
    }
}

/// REST polling fallback after repeated WS failures. Keeps trying to re-establish the socket
/// every `WS_RETRY_WHILE_POLLING`; returns to the caller either way so the outer loop retries.
async fn run_fallback_polling(
    hub: &Arc<MarketHub>,
    provider: &Arc<dyn MarketProvider>,
    symbols: &mut Vec<String>,
    cmd_rx: &mut mpsc::UnboundedReceiver<WsCommand>,
) {
    // `interval` fires its first tick immediately: an unshifted `retry_tick` would be ready at
    // t=0 alongside `poll_tick`, so `select!` would return from polling mode within a tick or
    // two and the REST fallback would never actually poll. `interval_at` delays the first tick
    // by a full period; `poll_tick` keeps its immediate tick on purpose (poll right away).
    let mut poll_tick = tokio::time::interval(FALLBACK_POLL_INTERVAL);
    let mut retry_tick = tokio::time::interval_at(
        tokio::time::Instant::now() + WS_RETRY_WHILE_POLLING,
        WS_RETRY_WHILE_POLLING,
    );
    loop {
        tokio::select! {
            _ = poll_tick.tick() => {
                if symbols.is_empty() {
                    continue;
                }
                match provider.poll_tickers(symbols).await {
                    Ok(snapshots) => {
                        for snapshot in snapshots {
                            hub.apply_ticker(snapshot);
                        }
                    }
                    Err(e) => eprintln!("fallback poll failed: {e}"),
                }
            }
            _ = retry_tick.tick() => {
                return;
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(WsCommand::SetSymbols(s)) => {
                        let emptied = s.is_empty();
                        *symbols = s;
                        if emptied {
                            return;
                        }
                    }
                    None => return,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_starts_at_one_second_with_jitter() {
        let mut b = Backoff::new();
        let delay = b.next_delay();
        assert!(delay >= Duration::from_millis(800) && delay <= Duration::from_millis(1200));
    }

    #[test]
    fn backoff_grows_and_caps_at_thirty_seconds() {
        let mut b = Backoff::new();
        for _ in 0..20 {
            b.next_delay();
        }
        let delay = b.next_delay();
        assert!(delay <= Duration::from_secs_f64(36.0), "capped step should never exceed 30s + jitter");
    }

    #[test]
    fn parses_a_real_combined_ticker_frame() {
        // Trimmed sample of an actual `<symbol>@ticker` payload in combined-stream form.
        let raw = r#"{"stream":"btcusdt@ticker","data":{"e":"24hrTicker","E":1723713600000,
            "s":"BTCUSDT","p":"60.91000000","P":"0.097","c":"62967.94000000",
            "q":"1234567.89000000","o":"62907.03000000","h":"63100.00000000","l":"62500.00000000"}}"#;
        let env: CombinedEnvelope = serde_json::from_str(raw).expect("frame must parse");
        assert_eq!(env.data.symbol, "BTCUSDT");
        assert_eq!(env.data.last_price.parse::<f64>().unwrap(), 62967.94);
        assert_eq!(env.data.percent_24h.parse::<f64>().unwrap(), 0.097);
        assert_eq!(env.data.event_time, 1723713600000);
    }

    #[test]
    fn backoff_resets_to_first_step() {
        let mut b = Backoff::new();
        for _ in 0..5 {
            b.next_delay();
        }
        b.reset();
        let delay = b.next_delay();
        assert!(delay >= Duration::from_millis(800) && delay <= Duration::from_millis(1200));
    }
}
