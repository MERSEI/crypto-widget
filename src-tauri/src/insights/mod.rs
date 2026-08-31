//! Research: what is happening to a coin, and which projects are worth a look.
//!
//! Everything else in this app reports a fact — a price, a balance, a filled order. This
//! module reports an *opinion*, produced by an LLM over live web search results, and that
//! difference drives every design decision here:
//!
//! - **Nothing is invented locally.** The verdict, the catalysts and the risks come from one
//!   model call; the numbers next to them (market cap, rank, supply, ATH) come from CoinGecko.
//!   The two are kept in separate structs so a reader can always tell which half is measured
//!   and which half is argued.
//! - **Every claim carries a link.** `NewsItem::url` and `Source::url` are required — a
//!   headline the user cannot open is indistinguishable from a hallucinated one.
//! - **A call costs money.** Results are cached on disk with a user-set TTL, refreshes are
//!   explicit, and `Usage` rides back with every answer so the panel can show what was spent.
//! - **The key never reaches the renderer.** It lives in the OS credential store like the
//!   futures and Etherscan keys — see `crate::secrets`.

pub mod ai;
pub mod coingecko;
pub mod commands;

use serde::{Deserialize, Serialize};

/// Credential-store namespace for the Anthropic API key.
pub const NAMESPACE: &str = "anthropic";

/// Default model. Deliberately the strongest one rather than the cheapest: this feature is
/// asked a question once and read for minutes, so answer quality dominates the per-call cost.
/// The user can move it down in settings — that is their call to make, not this file's.
pub const DEFAULT_MODEL: &str = "claude-opus-5";

/// Models offered in the settings dropdown, cheapest last. Kept here rather than in the
/// renderer so Rust stays the single source of truth for what it will actually accept.
pub const MODELS: [&str; 3] = ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"];

/// Persisted, non-secret half of the feature. `serde(default)` on the container for the same
/// reason as `WalletSettings`: a settings file written by an earlier build has none of these
/// keys, and a failed parse would back the whole file up and hand the user a blank watchlist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct InsightsSettings {
    /// Off until the user adds a key. A feature that spends money per click does not ship on.
    pub enabled: bool,
    pub model: String,
    /// How long a cached answer stays fresh. The unit is minutes because the useful range is
    /// "while I read it" (15) to "once a day" (1440), and every value in between is a
    /// deliberate trade of spend against staleness.
    pub cache_ttl_min: u32,
    /// Upper bound on the model's web searches per call — the main cost lever after the model.
    pub max_searches: u32,
    /// Language the answer is written in. Two-letter code, passed to the model as an
    /// instruction; anything it understands works, the UI offers en/ru.
    pub language: String,
}

impl Default for InsightsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            model: DEFAULT_MODEL.to_string(),
            cache_ttl_min: 120,
            max_searches: 6,
            language: "en".to_string(),
        }
    }
}

impl InsightsSettings {
    /// Clamps user input to ranges the backend will actually honour. Called on the way in from
    /// `set_insights_settings`, so a stored value is always usable as-is.
    pub fn sanitised(mut self) -> Self {
        if !MODELS.contains(&self.model.as_str()) {
            self.model = DEFAULT_MODEL.to_string();
        }
        self.cache_ttl_min = self.cache_ttl_min.clamp(5, 10_080);
        self.max_searches = self.max_searches.clamp(1, 12);
        if self.language.trim().is_empty() {
            self.language = "en".to_string();
        }
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Bullish,
    Neutral,
    Bearish,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Sentiment {
    Positive,
    Neutral,
    Negative,
}

/// One headline. `url` is not optional on purpose — see the module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub source: String,
    /// As the model read it off the page: "2026-08-29", "3 days ago", or absent. Left as free
    /// text rather than parsed into a timestamp — a wrong date is worse than a vague one.
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default = "neutral_sentiment")]
    pub sentiment: Sentiment,
    /// One line on why this matters for the price.
    #[serde(default)]
    pub impact: String,
}

fn neutral_sentiment() -> Sentiment {
    Sentiment::Neutral
}

/// A page the model actually consulted, harvested from the web-search results rather than
/// from the model's prose — so the citation list cannot be fabricated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub title: String,
    pub url: String,
}

/// The measured half of a coin report: CoinGecko's numbers, untouched by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fundamentals {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub market_cap_usd: Option<f64>,
    pub market_cap_rank: Option<u32>,
    pub volume24h_usd: Option<f64>,
    pub circulating_supply: Option<f64>,
    pub max_supply: Option<f64>,
    pub ath_usd: Option<f64>,
    /// Percent below the all-time high, negative when under it — CoinGecko's own figure.
    pub ath_change_pct: Option<f64>,
    pub categories: Vec<String>,
    pub homepage: Option<String>,
    pub github: Option<String>,
    /// CoinGecko's community up-vote share, 0–100.
    pub sentiment_up_pct: Option<f64>,
}

/// What one model call cost, in the only units the API reports. Shown in the panel so the
/// spend is visible at the moment it happens rather than at the end of the month.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub web_searches: u32,
}

/// The model's half of a coin report. Parsed straight out of the JSON it is asked to return,
/// which is why every field carries a `serde(default)`: a missing key must degrade one section
/// of the card, never fail the whole call after it has already been paid for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Analysis {
    pub verdict: Verdict,
    /// 0–100. Not a price target and not a probability — a relative "how interesting is this
    /// right now" the model is asked to keep comparable across coins.
    pub score: u8,
    pub summary: String,
    pub catalysts: Vec<String>,
    pub risks: Vec<String>,
    pub news: Vec<NewsItem>,
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            verdict: Verdict::Neutral,
            score: 50,
            summary: String::new(),
            catalysts: Vec::new(),
            risks: Vec::new(),
            news: Vec::new(),
        }
    }
}

/// One coin report, as the renderer receives it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinInsight {
    /// The watchlist symbol this was requested for, e.g. `BTCUSDT`.
    pub symbol: String,
    /// Its base asset, e.g. `BTC` — what was actually researched.
    pub asset: String,
    pub analysis: Analysis,
    pub fundamentals: Option<Fundamentals>,
    pub sources: Vec<Source>,
    /// Unix milliseconds. The age of the *answer*, not of this delivery — a cached reply keeps
    /// the timestamp of the call that produced it.
    pub generated_at: i64,
    pub model: String,
    pub usage: Usage,
    /// True when this came off disk rather than from a call. Set at delivery, not stored.
    #[serde(default)]
    pub cached: bool,
}

/// A project the scan surfaced. `binance_symbol` is what makes it actionable: when the model
/// names something the widget can actually price, the card gets an "add to watchlist" button.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectIdea {
    pub name: String,
    pub symbol: String,
    pub category: String,
    pub thesis: String,
    pub catalyst: String,
    pub risk: String,
    /// "days", "weeks", "months" — free text, the model's own horizon for the thesis.
    pub horizon: String,
    /// 0–100, the model's own confidence in the thesis.
    pub conviction: u8,
    pub url: Option<String>,
    /// Filled in on this side by matching `symbol` against Binance's pair list, never by the
    /// model: a hallucinated ticker would otherwise become a watchlist row that never prices.
    pub binance_symbol: Option<String>,
}

impl Default for ProjectIdea {
    fn default() -> Self {
        Self {
            name: String::new(),
            symbol: String::new(),
            category: String::new(),
            thesis: String::new(),
            catalyst: String::new(),
            risk: String::new(),
            horizon: String::new(),
            conviction: 50,
            url: None,
            binance_symbol: None,
        }
    }
}

/// What the model returns for a market scan, before this side attaches trending data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanAnalysis {
    /// Two or three sentences on what the market is doing right now.
    pub narrative: String,
    pub ideas: Vec<ProjectIdea>,
}

/// CoinGecko's trending list — a measured popularity signal shown next to the model's ideas so
/// the two can disagree visibly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendingCoin {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub market_cap_rank: Option<u32>,
    pub binance_symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketScan {
    pub narrative: String,
    pub ideas: Vec<ProjectIdea>,
    pub trending: Vec<TrendingCoin>,
    pub sources: Vec<Source>,
    pub generated_at: i64,
    pub model: String,
    pub usage: Usage,
    #[serde(default)]
    pub cached: bool,
}

/// Everything the settings screen needs. The key itself is represented by its
/// [`crate::secrets::CredentialStatus`] — present or not, masked — and never by its value.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsState {
    pub settings: InsightsSettings,
    pub key: crate::secrets::CredentialStatus,
    pub models: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_without_an_insights_section_still_loads() {
        let back: InsightsSettings = serde_json::from_str("{}").unwrap();
        assert!(!back.enabled, "a paid feature must arrive switched off");
        assert_eq!(back.model, DEFAULT_MODEL);
    }

    #[test]
    fn a_half_written_section_keeps_its_other_defaults() {
        // The failure mode this guards is the same one `WalletSettings` guards: one unknown
        // key missing, the whole settings.json declared corrupt, and the user's watchlist
        // replaced with defaults.
        let back: InsightsSettings = serde_json::from_str(r#"{"cacheTtlMin":30}"#).unwrap();
        assert_eq!(back.cache_ttl_min, 30);
        assert_eq!(back.max_searches, 6);
        assert_eq!(back.language, "en");
    }

    #[test]
    fn sanitising_rejects_a_model_the_backend_would_not_accept() {
        let settings = InsightsSettings {
            model: "gpt-4".into(),
            ..InsightsSettings::default()
        }
        .sanitised();
        assert_eq!(settings.model, DEFAULT_MODEL);
    }

    #[test]
    fn sanitising_clamps_the_cost_levers() {
        let settings = InsightsSettings {
            cache_ttl_min: 0,
            max_searches: 500,
            language: "  ".into(),
            ..InsightsSettings::default()
        }
        .sanitised();
        assert_eq!(settings.cache_ttl_min, 5, "a zero TTL would bill on every render");
        assert_eq!(settings.max_searches, 12);
        assert_eq!(settings.language, "en");
    }

    #[test]
    fn an_analysis_missing_every_optional_section_still_parses() {
        // Half a card is worth showing; a parse failure after the call has been billed is not.
        let analysis: Analysis = serde_json::from_str(r#"{"verdict":"bullish","score":71}"#).unwrap();
        assert_eq!(analysis.verdict, Verdict::Bullish);
        assert_eq!(analysis.score, 71);
        assert!(analysis.news.is_empty());
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = InsightsSettings {
            enabled: true,
            model: "claude-sonnet-5".into(),
            cache_ttl_min: 45,
            max_searches: 3,
            language: "ru".into(),
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("cacheTtlMin"), "the renderer reads camelCase: {json}");
        let back: InsightsSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, settings);
    }
}
