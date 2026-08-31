//! CoinGecko: the measured half of a coin report.
//!
//! The model argues; this file counts. Market cap, rank, supply, distance from the all-time
//! high and CoinGecko's own community sentiment come from here, get shown next to the model's
//! prose, and are also fed *into* the prompt — an analysis written without knowing a coin is
//! ranked 900th and 95% below its high is an analysis about a different asset.
//!
//! Two constraints shape the code:
//!
//! - **The free API is rate-limited and unauthenticated.** Every response is cached on disk,
//!   and a 429 falls back to the stale copy rather than failing the call — the fundamentals are
//!   a garnish on a report that is already paid for.
//! - **A ticker is not an identifier.** Dozens of listings share the symbol `GRAM`. The pick is
//!   made by [`pick_best`], which prefers an exact symbol match with the best market-cap rank,
//!   and is a pure function precisely so it can be tested without the network.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

const BASE: &str = "https://api.coingecko.com/api/v3";
const TIMEOUT: Duration = Duration::from_secs(12);

/// Fundamentals move slowly; the ticker in the row above them does not. Half an hour keeps the
/// call count far under the free tier's budget without ever showing a stale rank that matters.
const DETAIL_TTL: Duration = Duration::from_secs(30 * 60);
const TRENDING_TTL: Duration = Duration::from_secs(30 * 60);
/// A symbol keeps its CoinGecko id essentially forever, so this only exists to catch a
/// re-listing — and to keep the lookup out of the hot path on every refresh.
const ID_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub struct CoinGecko {
    http: reqwest::Client,
    cache_dir: PathBuf,
}

/// One row of `/search`, trimmed to what the pick needs.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchCoin {
    pub id: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub market_cap_rank: Option<u32>,
}

/// Chooses which listing a ticker means: an exact symbol match wins over a name match, and
/// among equals the best market-cap rank wins. An unranked coin loses to any ranked one —
/// CoinGecko lists thousands of dead tokens whose symbol collides with a live asset.
pub fn pick_best(coins: &[SearchCoin], asset: &str) -> Option<String> {
    let wanted = asset.to_uppercase();
    let mut exact: Vec<&SearchCoin> = coins
        .iter()
        .filter(|c| c.symbol.to_uppercase() == wanted)
        .collect();
    if exact.is_empty() {
        exact = coins.iter().collect();
    }
    exact
        .into_iter()
        .min_by_key(|c| c.market_cap_rank.unwrap_or(u32::MAX))
        .map(|c| c.id.clone())
}

impl CoinGecko {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .unwrap_or_default(),
            cache_dir,
        }
    }

    fn cache_path(&self, name: &str) -> PathBuf {
        self.cache_dir.join(format!("coingecko-{name}.json"))
    }

    /// Resolves `BTC` to `bitcoin`. `None` when the API is unreachable and nothing is cached —
    /// which costs the report its fundamentals section and nothing else.
    pub async fn resolve_id(&self, asset: &str) -> Option<String> {
        let key = asset.to_uppercase();
        let path = self.cache_path(&format!("id-{}", key.to_lowercase()));
        if let Some(id) = crate::config::cache::read_if_fresh::<String>(&path, ID_TTL) {
            return Some(id);
        }

        let url = format!("{BASE}/search?query={key}");
        let fetched = async {
            let response = self.http.get(&url).send().await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let body: SearchResponse = response.json().await.ok()?;
            pick_best(&body.coins, &key)
        }
        .await;

        match fetched {
            Some(id) => {
                crate::config::cache::write(&path, &id);
                Some(id)
            }
            // Rate limited or offline: an expired id is still the right id.
            None => crate::config::cache::read_stale::<String>(&path),
        }
    }

    pub async fn fundamentals(&self, asset: &str) -> Option<super::Fundamentals> {
        let id = self.resolve_id(asset).await?;
        let path = self.cache_path(&format!("coin-{id}"));
        if let Some(cached) = crate::config::cache::read_if_fresh::<super::Fundamentals>(&path, DETAIL_TTL) {
            return Some(cached);
        }

        let url = format!(
            "{BASE}/coins/{id}?localization=false&tickers=false&market_data=true&community_data=true&developer_data=false&sparkline=false"
        );
        let fetched = async {
            let response = self.http.get(&url).send().await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let detail: CoinDetail = response.json().await.ok()?;
            Some(detail.into_fundamentals())
        }
        .await;

        match fetched {
            Some(fundamentals) => {
                crate::config::cache::write(&path, &fundamentals);
                Some(fundamentals)
            }
            None => crate::config::cache::read_stale::<super::Fundamentals>(&path),
        }
    }

    /// The trending list, with `binance_symbol` left unset — only the caller knows which pairs
    /// this build can actually price.
    pub async fn trending(&self) -> Vec<super::TrendingCoin> {
        let path = self.cache_path("trending");
        if let Some(cached) = crate::config::cache::read_if_fresh::<Vec<super::TrendingCoin>>(&path, TRENDING_TTL) {
            return cached;
        }

        let fetched = async {
            let response = self.http.get(format!("{BASE}/search/trending")).send().await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let body: TrendingResponse = response.json().await.ok()?;
            Some(
                body.coins
                    .into_iter()
                    .map(|entry| super::TrendingCoin {
                        id: entry.item.id,
                        name: entry.item.name,
                        symbol: entry.item.symbol.to_uppercase(),
                        market_cap_rank: entry.item.market_cap_rank,
                        binance_symbol: None,
                    })
                    .collect::<Vec<_>>(),
            )
        }
        .await;

        match fetched {
            Some(coins) => {
                crate::config::cache::write(&path, &coins);
                coins
            }
            None => crate::config::cache::read_stale::<Vec<super::TrendingCoin>>(&path).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    coins: Vec<SearchCoin>,
}

#[derive(Debug, Deserialize)]
struct TrendingResponse {
    #[serde(default)]
    coins: Vec<TrendingEntry>,
}

#[derive(Debug, Deserialize)]
struct TrendingEntry {
    item: TrendingItem,
}

#[derive(Debug, Deserialize)]
struct TrendingItem {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    market_cap_rank: Option<u32>,
}

/// `/coins/{id}`, trimmed. Every field is optional: CoinGecko omits whole sections for small
/// listings, and a missing `max_supply` must not cost the report its market cap.
#[derive(Debug, Deserialize)]
struct CoinDetail {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    categories: Vec<Option<String>>,
    #[serde(default)]
    links: Option<Links>,
    #[serde(default)]
    market_data: Option<MarketData>,
    #[serde(default)]
    sentiment_votes_up_percentage: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Links {
    #[serde(default)]
    homepage: Vec<String>,
    #[serde(default)]
    repos_url: Option<ReposUrl>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReposUrl {
    #[serde(default)]
    github: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MarketData {
    #[serde(default)]
    market_cap: Option<CurrencyMap>,
    #[serde(default)]
    total_volume: Option<CurrencyMap>,
    #[serde(default)]
    ath: Option<CurrencyMap>,
    #[serde(default)]
    ath_change_percentage: Option<CurrencyMap>,
    #[serde(default)]
    market_cap_rank: Option<u32>,
    #[serde(default)]
    circulating_supply: Option<f64>,
    #[serde(default)]
    max_supply: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CurrencyMap {
    #[serde(default)]
    usd: Option<f64>,
}

impl CoinDetail {
    fn into_fundamentals(self) -> super::Fundamentals {
        let market = self.market_data;
        let usd = |field: &Option<CurrencyMap>| field.as_ref().and_then(|m| m.usd);
        super::Fundamentals {
            id: self.id,
            name: self.name,
            symbol: self.symbol.to_uppercase(),
            market_cap_usd: market.as_ref().and_then(|m| usd(&m.market_cap)),
            market_cap_rank: market.as_ref().and_then(|m| m.market_cap_rank),
            volume24h_usd: market.as_ref().and_then(|m| usd(&m.total_volume)),
            circulating_supply: market.as_ref().and_then(|m| m.circulating_supply),
            max_supply: market.as_ref().and_then(|m| m.max_supply),
            ath_usd: market.as_ref().and_then(|m| usd(&m.ath)),
            ath_change_pct: market.as_ref().and_then(|m| usd(&m.ath_change_percentage)),
            // Nulls appear in this array for de-listed categories; they are not categories.
            categories: self.categories.into_iter().flatten().take(4).collect(),
            homepage: self
                .links
                .as_ref()
                .and_then(|l| l.homepage.iter().find(|url| !url.trim().is_empty()).cloned()),
            github: self
                .links
                .as_ref()
                .and_then(|l| l.repos_url.as_ref())
                .and_then(|r| r.github.iter().find(|url| !url.trim().is_empty()).cloned()),
            sentiment_up_pct: self.sentiment_votes_up_percentage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coin(id: &str, symbol: &str, rank: Option<u32>) -> SearchCoin {
        SearchCoin {
            id: id.into(),
            symbol: symbol.into(),
            market_cap_rank: rank,
        }
    }

    #[test]
    fn an_exact_symbol_match_beats_a_name_match() {
        let coins = vec![
            coin("bitcoin-cash-token", "BCHT", Some(300)),
            coin("bitcoin", "BTC", Some(1)),
        ];
        assert_eq!(pick_best(&coins, "BTC").as_deref(), Some("bitcoin"));
    }

    #[test]
    fn among_colliding_tickers_the_ranked_one_wins() {
        // The failure this prevents: a dead 2021 token called GRAM outranking the live listing
        // in the search results and quietly turning the report into a report on something else.
        let coins = vec![
            coin("gram-dead", "GRAM", None),
            coin("gram-real", "GRAM", Some(420)),
        ];
        assert_eq!(pick_best(&coins, "GRAM").as_deref(), Some("gram-real"));
    }

    #[test]
    fn with_no_exact_match_the_best_ranked_result_is_used() {
        let coins = vec![coin("wrapped-thing", "WTHING", Some(90))];
        assert_eq!(pick_best(&coins, "THING").as_deref(), Some("wrapped-thing"));
    }

    #[test]
    fn an_empty_search_resolves_to_nothing() {
        assert_eq!(pick_best(&[], "BTC"), None);
    }

    #[test]
    fn a_detail_response_missing_every_section_still_maps() {
        // Small listings come back with no `market_data` and no `links` at all. That has to
        // yield a sparse card, not a failed lookup.
        let detail: CoinDetail = serde_json::from_str(r#"{"id":"tiny","name":"Tiny","symbol":"tny"}"#).unwrap();
        let fundamentals = detail.into_fundamentals();
        assert_eq!(fundamentals.symbol, "TNY");
        assert!(fundamentals.market_cap_usd.is_none());
        assert!(fundamentals.categories.is_empty());
    }

    #[test]
    fn usd_figures_and_links_are_lifted_out_of_their_nesting() {
        let detail: CoinDetail = serde_json::from_str(
            r#"{
                "id":"bitcoin","name":"Bitcoin","symbol":"btc",
                "categories":["Layer 1",null,"Smart Contract Platform"],
                "links":{"homepage":["","https://bitcoin.org"],"repos_url":{"github":["https://github.com/bitcoin/bitcoin"]}},
                "market_data":{
                    "market_cap":{"usd":1200000000000.0},
                    "total_volume":{"usd":45000000000.0},
                    "ath":{"usd":109000.0},
                    "ath_change_percentage":{"usd":-12.5},
                    "market_cap_rank":1,
                    "circulating_supply":19800000.0,
                    "max_supply":21000000.0
                },
                "sentiment_votes_up_percentage":78.4
            }"#,
        )
        .unwrap();
        let fundamentals = detail.into_fundamentals();

        assert_eq!(fundamentals.market_cap_rank, Some(1));
        assert_eq!(fundamentals.ath_change_pct, Some(-12.5));
        assert_eq!(fundamentals.max_supply, Some(21_000_000.0));
        assert_eq!(fundamentals.homepage.as_deref(), Some("https://bitcoin.org"), "an empty first entry is not a homepage");
        assert_eq!(fundamentals.categories, vec!["Layer 1", "Smart Contract Platform"]);
        assert_eq!(fundamentals.sentiment_up_pct, Some(78.4));
    }
}
