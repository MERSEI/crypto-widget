pub mod binance_rest;
pub mod binance_ws;
pub mod fx;
pub mod hub;
pub mod provider;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerSnapshot {
    pub symbol: String,
    pub price: f64,
    pub percent_24h: f64,
    pub quote_volume: f64,
    pub event_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub time: i64, // seconds, unix
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairInfo {
    pub symbol: String,
    pub base_asset: String,
    pub quote_volume: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Connecting,
    Live,
    Stale,
    Reconnecting,
    Polling,
    Offline,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub state: ConnectionState,
    pub attempt: u32,
    pub latency_ms: Option<u64>,
}
