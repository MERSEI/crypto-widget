use super::{Candle, PairInfo, TickerSnapshot};
use async_trait::async_trait;

#[async_trait]
pub trait MarketProvider: Send + Sync {
    async fn search_pairs(&self, query: &str) -> Result<Vec<PairInfo>, String>;
    async fn klines(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Candle>, String>;
    async fn poll_tickers(&self, symbols: &[String]) -> Result<Vec<TickerSnapshot>, String>;
}
