//! Backup price venues.
//!
//! Binance is the primary feed, but a pair can stop trading there while the coin itself is
//! perfectly alive — `TONUSDT` sat in `BREAK` status for weeks handing out the price of its
//! last trade, and the widget showed that frozen number as if it were live. Whenever the
//! primary feed goes quiet for a symbol, these venues are asked for the same pair in order
//! until one answers.
//!
//! Every venue here quotes real USDT (or whatever the pair's own quote asset is) spot markets,
//! addressed by base+quote. No index or aggregator sits in this list on purpose: an aggregator
//! is keyed by ticker symbol, and ticker symbols get reused — CoinGecko's `TON` is Tokamak
//! Network these days, so a symbol lookup would have answered "TONUSDT = 0.27" with total
//! confidence.

use super::ratelimit::WeightLimiter;
use super::{now_ms, split_pair, TickerSnapshot};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// Order matters: first venue that answers wins.
const VENUES: [Venue; 4] = [Venue::Okx, Venue::Bybit, Venue::Kucoin, Venue::Gate];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Venue {
    Okx,
    Bybit,
    Kucoin,
    Gate,
}

impl Venue {
    pub fn name(self) -> &'static str {
        match self {
            Venue::Okx => "okx",
            Venue::Bybit => "bybit",
            Venue::Kucoin => "kucoin",
            Venue::Gate => "gate",
        }
    }

    fn url(self, base: &str, quote: &str) -> String {
        match self {
            Venue::Okx => format!("https://www.okx.com/api/v5/market/ticker?instId={base}-{quote}"),
            Venue::Bybit => {
                format!("https://api.bybit.com/v5/market/tickers?category=spot&symbol={base}{quote}")
            }
            Venue::Kucoin => {
                format!("https://api.kucoin.com/api/v1/market/stats?symbol={base}-{quote}")
            }
            Venue::Gate => {
                format!("https://api.gateio.ws/api/v4/spot/tickers?currency_pair={base}_{quote}")
            }
        }
    }

    fn parse(self, body: &str) -> Option<Quote> {
        match self {
            Venue::Okx => parse_okx(body),
            Venue::Bybit => parse_bybit(body),
            Venue::Kucoin => parse_kucoin(body),
            Venue::Gate => parse_gate(body),
        }
    }
}

/// The three numbers every venue can give us, normalised.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quote {
    pub price: f64,
    pub percent_24h: f64,
    pub quote_volume: f64,
}

pub struct BackupProvider {
    client: reqwest::Client,
    /// Which venue last answered for a symbol, so the next refresh starts there instead of
    /// walking the whole list again. Purely an optimisation — a miss just falls through.
    preferred: Mutex<HashMap<String, Venue>>,
    /// None of these venues publish a documented public-tier limit as low as Binance's, but
    /// none are asked to prove it either — a shared, conservative budget across all four keeps
    /// a watchlist full of stale symbols from turning into a burst that gets this IP throttled.
    limiter: WeightLimiter,
}

impl BackupProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .expect("reqwest client"),
            preferred: Mutex::new(HashMap::new()),
            limiter: WeightLimiter::new("backup", 10.0, 5.0),
        }
    }

    /// Asks each venue in turn for `symbol` and returns the first answer, stamped with the
    /// current time (these are one-shot REST reads, so "now" is the event time) and the venue
    /// name. `None` means no backup venue lists the pair either.
    pub async fn fetch(&self, symbol: &str) -> Option<TickerSnapshot> {
        let (base, quote) = split_pair(symbol)?;

        let first = self.preferred.lock().unwrap().get(symbol).copied();
        let order = first
            .into_iter()
            .chain(VENUES.into_iter().filter(|v| Some(*v) != first));

        for venue in order {
            let Some(quote_data) = self.fetch_one(venue, &base, &quote).await else {
                continue;
            };
            self.preferred
                .lock()
                .unwrap()
                .insert(symbol.to_string(), venue);
            return Some(TickerSnapshot {
                symbol: symbol.to_uppercase(),
                price: quote_data.price,
                percent_24h: quote_data.percent_24h,
                quote_volume: quote_data.quote_volume,
                event_time: now_ms(),
                source: venue.name().into(),
            });
        }
        None
    }

    async fn fetch_one(&self, venue: Venue, base: &str, quote: &str) -> Option<Quote> {
        if let Err(e) = self.limiter.take(1.0) {
            eprintln!("backup {}: {e}", venue.name());
            return None;
        }

        let response = match self.client.get(venue.url(base, quote)).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("backup {} request failed: {e}", venue.name());
                return None;
            }
        };
        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("backup {} returned an error status: {e}", venue.name());
                return None;
            }
        };
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("backup {} body read failed: {e}", venue.name());
                return None;
            }
        };
        // A pair the venue doesn't list comes back as a well-formed error body, not an HTTP
        // error, on most of these APIs — so parsing failure is the "not here" signal, not
        // logged as one.
        venue.parse(&body).filter(|q| q.price > 0.0)
    }
}

/// How often the watchlist is swept for symbols the primary feed has gone quiet on.
const SWEEP_INTERVAL: Duration = Duration::from_secs(20);

/// Background task: keeps every watchlist symbol on a live price, whoever has to provide it.
/// Only symbols whose primary snapshot is missing or stale are touched, so a normal session
/// with a healthy WS never sends a single backup request.
pub fn spawn_watcher(
    hub: std::sync::Arc<super::hub::MarketHub>,
    config: std::sync::Arc<crate::config::ConfigState>,
    backup: std::sync::Arc<BackupProvider>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            interval.tick().await;
            let symbols: Vec<String> = config
                .snapshot()
                .watchlist
                .iter()
                .map(|w| w.symbol.clone())
                .collect();
            for symbol in symbols {
                if !hub.needs_backup(&symbol) {
                    continue;
                }
                if let Some(snapshot) = backup.fetch(&symbol).await {
                    hub.apply_ticker(snapshot);
                }
            }
        }
    });
}

fn num(raw: &str) -> f64 {
    raw.parse().unwrap_or(0.0)
}

#[derive(Deserialize)]
struct OkxEnvelope {
    data: Vec<OkxTicker>,
}

#[derive(Deserialize)]
struct OkxTicker {
    last: String,
    open24h: String,
    #[serde(rename = "volCcy24h")]
    vol_ccy_24h: String,
}

fn parse_okx(body: &str) -> Option<Quote> {
    let envelope: OkxEnvelope = serde_json::from_str(body).ok()?;
    let t = envelope.data.into_iter().next()?;
    let (last, open) = (num(&t.last), num(&t.open24h));
    Some(Quote {
        price: last,
        // OKX reports the open, not the change, so the percentage is ours to compute.
        percent_24h: if open > 0.0 { (last - open) / open * 100.0 } else { 0.0 },
        quote_volume: num(&t.vol_ccy_24h),
    })
}

#[derive(Deserialize)]
struct BybitEnvelope {
    result: BybitResult,
}

#[derive(Deserialize)]
struct BybitResult {
    list: Vec<BybitTicker>,
}

#[derive(Deserialize)]
struct BybitTicker {
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "price24hPcnt")]
    price_24h_pcnt: String,
    #[serde(rename = "turnover24h")]
    turnover_24h: String,
}

fn parse_bybit(body: &str) -> Option<Quote> {
    let envelope: BybitEnvelope = serde_json::from_str(body).ok()?;
    let t = envelope.result.list.into_iter().next()?;
    Some(Quote {
        price: num(&t.last_price),
        // Bybit's `price24hPcnt` is a ratio ("-0.0005"), not a percentage.
        percent_24h: num(&t.price_24h_pcnt) * 100.0,
        quote_volume: num(&t.turnover_24h),
    })
}

#[derive(Deserialize)]
struct KucoinEnvelope {
    data: Option<KucoinStats>,
}

#[derive(Deserialize)]
struct KucoinStats {
    last: Option<String>,
    #[serde(rename = "changeRate")]
    change_rate: Option<String>,
    #[serde(rename = "volValue")]
    vol_value: Option<String>,
}

fn parse_kucoin(body: &str) -> Option<Quote> {
    let envelope: KucoinEnvelope = serde_json::from_str(body).ok()?;
    // KuCoin answers `{"code":"200000","data":null}` for an unknown pair, and every field of
    // `data` is nullable even when it isn't.
    let stats = envelope.data?;
    Some(Quote {
        price: num(&stats.last?),
        percent_24h: stats.change_rate.map(|r| num(&r) * 100.0).unwrap_or(0.0),
        quote_volume: stats.vol_value.map(|v| num(&v)).unwrap_or(0.0),
    })
}

#[derive(Deserialize)]
struct GateTicker {
    last: String,
    change_percentage: String,
    quote_volume: String,
}

fn parse_gate(body: &str) -> Option<Quote> {
    let tickers: Vec<GateTicker> = serde_json::from_str(body).ok()?;
    let t = tickers.into_iter().next()?;
    Some(Quote {
        price: num(&t.last),
        percent_24h: num(&t.change_percentage),
        quote_volume: num(&t.quote_volume),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim bodies from each venue for SOL-USDT, trimmed to the fields we read.
    #[test]
    fn parses_okx_ticker() {
        let body = r#"{"code":"0","msg":"","data":[{"instId":"SOL-USDT","last":"75.24",
            "open24h":"75.29","volCcy24h":"11578511.72","vol24h":"153412.40"}]}"#;
        let q = parse_okx(body).unwrap();
        assert!((q.price - 75.24).abs() < 1e-9);
        assert!(q.percent_24h < 0.0, "a lower last than open must read as a fall");
        assert!((q.quote_volume - 11578511.72).abs() < 1e-6);
    }

    #[test]
    fn parses_bybit_ratio_as_percent() {
        let body = r#"{"retCode":0,"result":{"category":"spot","list":[{"symbol":"SOLUSDT",
            "lastPrice":"75.24","price24hPcnt":"-0.0005","turnover24h":"6947160.73"}]}}"#;
        let q = parse_bybit(body).unwrap();
        assert!((q.percent_24h - (-0.05)).abs() < 1e-9, "ratio must be scaled to percent");
    }

    #[test]
    fn parses_kucoin_stats() {
        let body = r#"{"code":"200000","data":{"symbol":"SOL-USDT","last":"75.23",
            "changeRate":"-0.0007","volValue":"7310018.39"}}"#;
        let q = parse_kucoin(body).unwrap();
        assert!((q.price - 75.23).abs() < 1e-9);
        assert!((q.percent_24h - (-0.07)).abs() < 1e-9);
    }

    #[test]
    fn kucoin_null_data_is_not_a_price() {
        assert!(parse_kucoin(r#"{"code":"200000","data":null}"#).is_none());
    }

    #[test]
    fn parses_gate_ticker() {
        let body = r#"[{"currency_pair":"SOL_USDT","last":"75.24","change_percentage":"-0.03",
            "quote_volume":"10909354.50"}]"#;
        let q = parse_gate(body).unwrap();
        assert!((q.percent_24h - (-0.03)).abs() < 1e-9);
    }

    #[test]
    fn delisted_pair_bodies_yield_nothing() {
        assert!(parse_gate(r#"{"label":"INVALID_CURRENCY","message":"currency TON is delisted"}"#).is_none());
        assert!(parse_bybit(r#"{"retCode":10001,"retMsg":"Not supported symbols","result":{}}"#).is_none());
        assert!(parse_okx(r#"{"code":"51001","msg":"doesn't exist","data":[]}"#).is_none());
    }
}
