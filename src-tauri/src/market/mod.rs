pub mod backup;
pub mod binance_rest;
pub mod binance_ws;
pub mod fx;
pub mod hub;
pub mod provider;

use serde::{Deserialize, Serialize};

/// A price older than this is not a price any more: no alert may fire on it and the row is
/// marked in the UI. Binance pushes a `@ticker` frame roughly once a second for a live pair,
/// so a minute of silence means the pair stopped trading (`BREAK`) or the feed is broken.
pub const STALE_AFTER_MS: i64 = 60_000;

/// Quote assets the widget knows how to split off a Binance-style symbol. Longest first, so
/// `GRAMUSDT` splits as GRAM/USDT and not GRAMU/SDT.
const KNOWN_QUOTES: [&str; 6] = ["FDUSD", "USDT", "USDC", "USD1", "TRY", "BTC"];

/// Splits `GRAMUSDT` into `("GRAM", "USDT")`. Backup venues all address a pair by its two
/// halves, so the split has to happen before any of them can be asked for a price.
pub fn split_pair(symbol: &str) -> Option<(String, String)> {
    let upper = symbol.to_uppercase();
    KNOWN_QUOTES
        .iter()
        .filter(|quote| upper.len() > quote.len() && upper.ends_with(*quote))
        .max_by_key(|quote| quote.len())
        .map(|quote| {
            let base = upper[..upper.len() - quote.len()].to_string();
            (base, (*quote).to_string())
        })
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerSnapshot {
    pub symbol: String,
    pub price: f64,
    pub percent_24h: f64,
    pub quote_volume: f64,
    pub event_time: i64,
    /// Which venue the number came from — "binance" for the primary feed, the backup venue's
    /// name when Binance had nothing live. Shown in the row so a substituted price is never
    /// mistaken for the exchange the pair was added from.
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "binance".into()
}

impl TickerSnapshot {
    pub fn is_stale(&self, now_ms: i64) -> bool {
        now_ms - self.event_time > STALE_AFTER_MS
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(event_time: i64) -> TickerSnapshot {
        TickerSnapshot {
            symbol: "TONUSDT".into(),
            price: 1.6,
            percent_24h: 0.0,
            quote_volume: 0.0,
            event_time,
            source: "binance".into(),
        }
    }

    #[test]
    fn splits_a_pair_into_base_and_quote() {
        assert_eq!(split_pair("GRAMUSDT"), Some(("GRAM".into(), "USDT".into())));
        assert_eq!(split_pair("BTCFDUSD"), Some(("BTC".into(), "FDUSD".into())));
        assert_eq!(split_pair("ethusdt"), Some(("ETH".into(), "USDT".into())));
    }

    #[test]
    fn refuses_symbols_with_no_known_quote() {
        assert_eq!(split_pair("BTCPLN"), None);
        // A bare quote asset is not a pair — the base half would be empty.
        assert_eq!(split_pair("USDT"), None);
    }

    #[test]
    fn a_price_older_than_the_cutoff_is_stale() {
        let now = 1_786_876_000_000;
        assert!(!snapshot(now - 5_000).is_stale(now));
        assert!(!snapshot(now - STALE_AFTER_MS).is_stale(now), "the cutoff itself still counts as live");
        assert!(snapshot(now - STALE_AFTER_MS - 1).is_stale(now));
        // The TONUSDT case: last trade weeks ago, price still being served.
        assert!(snapshot(now - 40 * 24 * 3600 * 1000).is_stale(now));
    }

    #[test]
    fn a_snapshot_without_a_source_reads_as_binance() {
        let parsed: TickerSnapshot = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","price":1.0,"percent24h":0.0,"quoteVolume":0.0,"eventTime":1}"#,
        )
        .expect("older cached snapshots must still load");
        assert_eq!(parsed.source, "binance");
    }
}
