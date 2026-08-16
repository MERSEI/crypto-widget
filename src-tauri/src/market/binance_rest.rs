use super::provider::MarketProvider;
use super::{Candle, PairInfo, TickerSnapshot};
use crate::config::cache;
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const BASE_URL: &str = "https://api.binance.com";

/// Simple token bucket: refills `rate` weight/sec, caps at `capacity`.
/// Guards against a runaway loop hammering Binance and getting the IP banned.
struct TokenBucket {
    tokens: f64,
    capacity: f64,
    rate_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, rate_per_sec: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            rate_per_sec,
            last_refill: Instant::now(),
        }
    }

    fn try_take(&mut self, cost: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate_per_sec).min(self.capacity);
        self.last_refill = now;
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

pub struct BinanceProvider {
    client: reqwest::Client,
    cache_dir: PathBuf,
    bucket: Mutex<TokenBucket>,
}

/// Parses Binance's kline response: an array of positional arrays whose first five slots are
/// open time + OHLC and whose tail we ignore.
///
/// This used to be a 12-field tuple struct with `#[serde(skip)]` on the six unused slots — but
/// a skipped tuple field does not consume its element, so serde expected a 6-element array,
/// hit twelve, and every single request failed with a length mismatch. That is what put
/// "Chart unavailable" on screen for every coin and timeframe. Reading rows as `Vec<Value>`
/// also means an extra column added by Binance later can't break charts again.
fn parse_klines(body: &str) -> Result<Vec<Candle>, String> {
    let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(rows.iter().filter_map(|row| row_to_candle(row)).collect())
}

fn row_to_candle(row: &[serde_json::Value]) -> Option<Candle> {
    let price = |index: usize| -> Option<f64> { row.get(index)?.as_str()?.parse().ok() };
    Some(Candle {
        time: row.first()?.as_i64()? / 1000,
        open: price(1)?,
        high: price(2)?,
        low: price(3)?,
        close: price(4)?,
    })
}

#[derive(Deserialize)]
struct Ticker24hr {
    symbol: String,
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "priceChangePercent")]
    price_change_percent: String,
    #[serde(rename = "quoteVolume")]
    quote_volume: String,
    #[serde(rename = "closeTime")]
    close_time: i64,
}

impl BinanceProvider {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
            cache_dir,
            // 6000 weight/min budget; keep a conservative local cap well under it.
            bucket: Mutex::new(TokenBucket::new(1200.0, 20.0)),
        }
    }

    fn take_weight(&self, weight: f64) -> Result<(), String> {
        if self.bucket.lock().unwrap().try_take(weight) {
            Ok(())
        } else {
            Err("local rate limit reached, backing off".into())
        }
    }

    async fn fetch_catalog(&self) -> Result<Vec<PairInfo>, String> {
        self.take_weight(80.0)?;
        let url = format!("{BASE_URL}/api/v3/ticker/24hr");
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let tickers: Vec<Ticker24hr> = resp.json().await.map_err(|e| e.to_string())?;
        let pairs = tickers
            .into_iter()
            .filter(|t| t.symbol.ends_with("USDT"))
            .map(|t| PairInfo {
                base_asset: t.symbol.trim_end_matches("USDT").to_string(),
                symbol: t.symbol,
                quote_volume: t.quote_volume.parse().unwrap_or(0.0),
            })
            .collect();
        Ok(pairs)
    }

    async fn catalog(&self) -> Result<Vec<PairInfo>, String> {
        let path = self.cache_dir.join("exchange_info.json");
        if let Some(cached) = cache::read_if_fresh::<Vec<PairInfo>>(&path, Duration::from_secs(24 * 3600)) {
            return Ok(cached);
        }
        match self.fetch_catalog().await {
            Ok(fresh) => {
                cache::write(&path, &fresh);
                Ok(fresh)
            }
            Err(e) => cache::read_stale::<Vec<PairInfo>>(&path).ok_or(e),
        }
    }
}

#[async_trait]
impl MarketProvider for BinanceProvider {
    async fn search_pairs(&self, query: &str) -> Result<Vec<PairInfo>, String> {
        let mut pairs = self.catalog().await?;
        let q = query.trim().to_uppercase();
        if !q.is_empty() {
            pairs.retain(|p| p.symbol.contains(&q) || p.base_asset.contains(&q));
        }
        // `"NaN".parse::<f64>()` succeeds, so an odd volume string from the API would reach
        // `unwrap()` here and panic inside the command — a dead search dialog, not an error.
        pairs.sort_by(|a, b| {
            b.quote_volume
                .partial_cmp(&a.quote_volume)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        pairs.truncate(50);
        Ok(pairs)
    }

    async fn klines(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Candle>, String> {
        self.take_weight(2.0)?;
        let url = format!(
            "{BASE_URL}/api/v3/klines?symbol={}&interval={}&limit={}",
            symbol.to_uppercase(),
            interval,
            limit.min(200)
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        parse_klines(&body)
    }

    async fn poll_tickers(&self, symbols: &[String]) -> Result<Vec<TickerSnapshot>, String> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }
        self.take_weight(2.0 * symbols.len() as f64)?;
        let symbols_json = serde_json::to_string(
            &symbols.iter().map(|s| s.to_uppercase()).collect::<Vec<_>>(),
        )
        .map_err(|e| e.to_string())?;
        let url = format!(
            "{BASE_URL}/api/v3/ticker/24hr?symbols={}",
            urlencoding_light(&symbols_json)
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let tickers: Vec<Ticker24hr> = resp.json().await.map_err(|e| e.to_string())?;
        Ok(tickers
            .into_iter()
            .map(|t| TickerSnapshot {
                symbol: t.symbol,
                price: t.last_price.parse().unwrap_or(0.0),
                percent_24h: t.price_change_percent.parse().unwrap_or(0.0),
                quote_volume: t.quote_volume.parse().unwrap_or(0.0),
                event_time: t.close_time,
            })
            .collect())
    }
}

/// Binance's `symbols=[...]` param needs URL-encoding of `[`, `]`, `"`, `,` — avoid pulling in
/// a whole percent-encoding crate for four characters.
fn urlencoding_light(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '[' => "%5B".to_string(),
            ']' => "%5D".to_string(),
            '"' => "%22".to_string(),
            ',' => "%2C".to_string(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One verbatim row from `/api/v3/klines?symbol=BTCUSDT&interval=4h&limit=1`.
    const REAL_KLINE_ROW: &str = r#"[[1786824000000,"63126.07000000","63175.00000000",
        "63076.96000000","63100.00000000","378.41943000",1786838399999,"23891963.09337220",
        63732,"208.71575000","13177510.59962130","0"]]"#;

    #[test]
    fn parses_a_real_kline_row() {
        let candles = parse_klines(REAL_KLINE_ROW).expect("a live Binance row must parse");
        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].time, 1786824000);
        assert_eq!(candles[0].open, 63126.07);
        assert_eq!(candles[0].high, 63175.0);
        assert_eq!(candles[0].low, 63076.96);
        assert_eq!(candles[0].close, 63100.0);
    }

    #[test]
    fn tolerates_extra_trailing_columns() {
        let with_extra = r#"[[1786824000000,"1.0","2.0","0.5","1.5","10.0",1,"2",3,"4","5","0","future"]]"#;
        let candles = parse_klines(with_extra).expect("an added column must not break the chart");
        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].close, 1.5);
    }

    #[test]
    fn skips_malformed_rows_instead_of_failing_the_batch() {
        let mixed = r#"[[1786824000000,"1.0","2.0","0.5","1.5","10.0"],["nope"]]"#;
        let candles = parse_klines(mixed).expect("a short row must not poison the response");
        assert_eq!(candles.len(), 1);
    }
}
