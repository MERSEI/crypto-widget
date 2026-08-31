//! What the renderer may ask the research module to do.
//!
//! Every other command file in this app answers from state it already has. These four spend
//! money, which changes the rules:
//!
//! - **Nothing calls the model implicitly.** There is no poll loop and no prefetch; a call
//!   happens because the user pressed a button, and a cached answer is served until they ask
//!   for a fresh one.
//! - **Only one call runs at a time.** [`call_guard`] refuses a second request rather than
//!   queueing it — a double-clicked button that bills twice is the failure this prevents.
//! - **The prompt is built here, from measured inputs.** The live price and CoinGecko's figures
//!   are pushed *into* the question so the answer is about the asset as it is right now, not as
//!   it was in the training data.
//! - **What comes back is sanitised before it is stored.** A model may return a 300-item list or
//!   a score of 4000; the panel and the cache only ever see values this file would render.

use super::ai::{self, AnthropicClient};
use super::coingecko::CoinGecko;
use super::{
    Analysis, CoinInsight, Fundamentals, InsightsSettings, InsightsState, MarketScan, ScanAnalysis,
    MODELS, NAMESPACE,
};
use crate::market::provider::MarketProvider;
use crate::{secrets, AppState};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, State};

/// Caps on what a single answer may carry into the cache and the panel. The model is asked for
/// less than this; the clamps exist because "asked for" is not "guaranteed", and an unbounded
/// list would be an unbounded card.
const MAX_BULLETS: usize = 6;
const MAX_NEWS: usize = 8;
const MAX_IDEAS: usize = 8;
const MAX_SOURCES: usize = 12;

/// Serialises the paid calls. A `Mutex` rather than a queue on purpose: a second request while
/// one is in flight is a double click or a second window, and the right answer to both is
/// "one is already running", not "here is a second bill".
fn call_guard() -> &'static tokio::sync::Mutex<()> {
    static GUARD: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(Default::default)
}

fn settings_of(state: &State<'_, AppState>) -> InsightsSettings {
    state.config.snapshot().insights
}

fn state_of(settings: &InsightsSettings) -> InsightsState {
    InsightsState {
        settings: settings.clone(),
        key: secrets::status_raw(NAMESPACE),
        models: MODELS.iter().map(|m| (*m).to_string()).collect(),
    }
}

/// The two things a paid call needs before it may run: the feature switched on and a key on
/// disk. Both messages name the fix, because both are states the user put the app into.
fn ready(settings: &InsightsSettings) -> Result<String, String> {
    if !settings.enabled {
        return Err("AI research is switched off — turn it on in the AI tab".into());
    }
    secrets::load_raw(NAMESPACE)
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| "no Anthropic API key stored — add one in the AI tab".into())
}

fn client_for(settings: &InsightsSettings, api_key: String) -> AnthropicClient {
    AnthropicClient::new(api_key, settings.model.clone(), settings.max_searches)
}

fn cache_path(cache_dir: &std::path::Path, name: &str) -> PathBuf {
    cache_dir.join(format!("insights-{name}.json"))
}

/// A language code as an instruction the model can act on. Anything outside the two the UI
/// offers is passed through as-is — the model understands far more languages than this widget
/// has any business enumerating.
fn language_clause(code: &str) -> String {
    match code.to_lowercase().as_str() {
        "en" => "Write every string in English.".to_string(),
        "ru" => "Write every string in Russian.".to_string(),
        other => format!("Write every string in the language with ISO code '{other}'."),
    }
}

#[tauri::command]
pub async fn get_insights_state(state: State<'_, AppState>) -> Result<InsightsState, String> {
    Ok(state_of(&settings_of(&state)))
}

#[tauri::command]
pub async fn set_insights_settings(
    state: State<'_, AppState>,
    enabled: bool,
    model: String,
    cache_ttl_min: u32,
    max_searches: u32,
    language: String,
) -> Result<InsightsState, String> {
    let sanitised = InsightsSettings {
        enabled,
        model,
        cache_ttl_min,
        max_searches,
        language,
    }
    .sanitised();

    {
        let mut guard = state.config.settings.write().unwrap();
        guard.insights = sanitised.clone();
    }
    state.config.mark_dirty();
    Ok(state_of(&sanitised))
}

/// Stores the Anthropic key. `save_raw`, not `save`: this is one opaque string, and filing it
/// as a key/secret pair would make `status_raw` — what `state_of` reads — report it missing.
#[tauri::command]
pub async fn set_anthropic_key(
    state: State<'_, AppState>,
    api_key: String,
) -> Result<InsightsState, String> {
    secrets::save_raw(NAMESPACE, &api_key)?;
    Ok(state_of(&settings_of(&state)))
}

#[tauri::command]
pub async fn clear_anthropic_key(state: State<'_, AppState>) -> Result<InsightsState, String> {
    secrets::delete(NAMESPACE)?;
    Ok(state_of(&settings_of(&state)))
}

/// The cached answer for a symbol, if there is one — free, and the only command the panel may
/// call on its own. Everything that costs money is behind an explicit button.
#[tauri::command]
pub async fn get_cached_insight(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<Option<CoinInsight>, String> {
    let settings = settings_of(&state);
    let asset = base_asset(&symbol);
    let path = cache_path(&state.config.cache_dir, &format!("coin-{}", asset.to_lowercase()));
    Ok(read_cached::<CoinInsight>(&path, settings.cache_ttl_min).map(mark_cached))
}

#[tauri::command]
pub async fn get_cached_scan(state: State<'_, AppState>) -> Result<Option<MarketScan>, String> {
    let settings = settings_of(&state);
    let path = cache_path(&state.config.cache_dir, "scan");
    Ok(read_cached::<MarketScan>(&path, settings.cache_ttl_min).map(mark_scan_cached))
}

/// Researches one watchlist symbol. `refresh` skips the cache — the only way to spend money on
/// a question that already has an answer.
#[tauri::command]
pub async fn research_coin(
    state: State<'_, AppState>,
    symbol: String,
    refresh: bool,
) -> Result<CoinInsight, String> {
    let settings = settings_of(&state);
    let asset = base_asset(&symbol);
    let path = cache_path(&state.config.cache_dir, &format!("coin-{}", asset.to_lowercase()));

    if !refresh {
        if let Some(cached) = read_cached::<CoinInsight>(&path, settings.cache_ttl_min) {
            return Ok(mark_cached(cached));
        }
    }

    let api_key = ready(&settings)?;
    let guard = call_guard()
        .try_lock()
        .map_err(|_| "another research call is still running".to_string())?;

    // Both halves of the prompt's context, fetched before the paid call so a slow CoinGecko
    // costs a few seconds rather than a wasted answer.
    let gecko = CoinGecko::new(state.config.cache_dir.clone());
    let fundamentals = gecko.fundamentals(&asset).await;
    let quote = state
        .hub
        .tickers_snapshot()
        .into_iter()
        .find(|t| t.symbol.eq_ignore_ascii_case(&symbol));

    let client = client_for(&settings, api_key);
    let reply = client
        .ask(
            &coin_system(&settings.language),
            &coin_user(&asset, &symbol, fundamentals.as_ref(), quote.as_ref()),
        )
        .await?;

    let analysis = parse_analysis(&reply.text)?;
    let insight = CoinInsight {
        symbol: symbol.to_uppercase(),
        asset,
        analysis,
        fundamentals,
        sources: reply.sources.into_iter().take(MAX_SOURCES).collect(),
        generated_at: crate::market::now_ms(),
        model: settings.model.clone(),
        usage: reply.usage,
        cached: false,
    };

    crate::config::cache::write(&path, &insight);
    drop(guard);
    Ok(insight)
}

/// Scans the market for projects worth a look. Not symbol-scoped, so it has one cache entry and
/// one refresh button.
#[tauri::command]
pub async fn research_market(
    state: State<'_, AppState>,
    refresh: bool,
) -> Result<MarketScan, String> {
    let settings = settings_of(&state);
    let path = cache_path(&state.config.cache_dir, "scan");

    if !refresh {
        if let Some(cached) = read_cached::<MarketScan>(&path, settings.cache_ttl_min) {
            return Ok(mark_scan_cached(cached));
        }
    }

    let api_key = ready(&settings)?;
    let guard = call_guard()
        .try_lock()
        .map_err(|_| "another research call is still running".to_string())?;

    let gecko = CoinGecko::new(state.config.cache_dir.clone());
    let trending = gecko.trending().await;
    let held: Vec<String> = state
        .config
        .snapshot()
        .watchlist
        .iter()
        .map(|item| base_asset(&item.symbol))
        .collect();

    let client = client_for(&settings, api_key);
    let reply = client
        .ask(
            &scan_system(&settings.language),
            &scan_user(&held, &trending),
        )
        .await?;

    let parsed = parse_scan(&reply.text)?;
    let provider = state.provider.clone();

    let mut ideas = Vec::new();
    for mut idea in parsed.ideas.into_iter().take(MAX_IDEAS) {
        idea.conviction = idea.conviction.min(100);
        // Attached here, never taken from the model: an invented ticker would otherwise become
        // an "add to watchlist" button that adds a row Binance never prices.
        idea.binance_symbol = resolve_binance_symbol(provider.as_ref(), &idea.symbol).await;
        ideas.push(idea);
    }

    let mut trending = trending;
    for coin in &mut trending {
        coin.binance_symbol = resolve_binance_symbol(provider.as_ref(), &coin.symbol).await;
    }

    let scan = MarketScan {
        narrative: parsed.narrative,
        ideas,
        trending,
        sources: reply.sources.into_iter().take(MAX_SOURCES).collect(),
        generated_at: crate::market::now_ms(),
        model: settings.model.clone(),
        usage: reply.usage,
        cached: false,
    };

    crate::config::cache::write(&path, &scan);
    drop(guard);
    Ok(scan)
}

/// Opens a link from a report in the system browser.
///
/// Deliberately not a general "open this URL" command. Every string here was written by a model
/// or lifted out of a search result, so the rule the referral panel already follows applies with
/// more force: the app opens a URL because it can point at the report the URL came from, never
/// because the renderer asked. A link the cache cannot vouch for is refused.
#[tauri::command]
pub async fn open_insight_url(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|_| format!("not a valid URL: {url}"))?;
    if parsed.scheme() != "https" || !parsed.has_host() {
        return Err("only https links from a report can be opened".into());
    }
    if !is_reported_url(&state.config.cache_dir, &url) {
        return Err("refusing to open a link that is not in a stored report".into());
    }

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("could not open the browser: {e}"))
}

// ---------------------------------------------------------------------------------------------
// Pure helpers. Everything below is testable without a network, a key, or an app handle.
// ---------------------------------------------------------------------------------------------

/// True when `url` appears verbatim in a stored report — as a citation, a headline's link, an
/// idea's page, or a project's own homepage. Compared field by field rather than by searching
/// the file text, so a URL that merely shares a prefix with a stored one does not pass.
fn is_reported_url(cache_dir: &std::path::Path, url: &str) -> bool {
    if let Some(scan) = crate::config::cache::read_stale::<MarketScan>(&cache_path(cache_dir, "scan")) {
        let in_scan = scan.sources.iter().any(|s| s.url == url)
            || scan.ideas.iter().any(|i| i.url.as_deref() == Some(url));
        if in_scan {
            return true;
        }
    }

    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        if !entry.file_name().to_string_lossy().starts_with("insights-coin-") {
            return false;
        }
        crate::config::cache::read_stale::<CoinInsight>(&entry.path()).is_some_and(|insight| {
            insight.sources.iter().any(|s| s.url == url)
                || insight.analysis.news.iter().any(|n| n.url == url)
                || insight
                    .fundamentals
                    .as_ref()
                    .is_some_and(|f| {
                        f.homepage.as_deref() == Some(url) || f.github.as_deref() == Some(url)
                    })
        })
    })
}

/// `BTCUSDT` → `BTC`. A symbol with no known quote suffix is already an asset name — the panel
/// can be asked about `BTC` as easily as about the pair.
pub fn base_asset(symbol: &str) -> String {
    crate::market::split_pair(symbol)
        .map(|(base, _)| base)
        .unwrap_or_else(|| symbol.trim().to_uppercase())
}

fn read_cached<T: serde::de::DeserializeOwned>(path: &std::path::Path, ttl_min: u32) -> Option<T> {
    crate::config::cache::read_if_fresh::<T>(path, Duration::from_secs(u64::from(ttl_min) * 60))
}

fn mark_cached(mut insight: CoinInsight) -> CoinInsight {
    insight.cached = true;
    insight
}

fn mark_scan_cached(mut scan: MarketScan) -> MarketScan {
    scan.cached = true;
    scan
}

/// Looks up the pair this build could actually price for a bare ticker. `None` when Binance has
/// no USDT market for it — which is the honest answer, not a failure.
async fn resolve_binance_symbol(provider: &dyn MarketProvider, ticker: &str) -> Option<String> {
    let ticker = ticker.trim().to_uppercase();
    if ticker.is_empty() {
        return None;
    }
    let wanted = format!("{ticker}USDT");
    let pairs = provider.search_pairs(&wanted).await.ok()?;
    pairs
        .into_iter()
        .find(|pair| pair.symbol == wanted)
        .map(|pair| pair.symbol)
}

/// Parses the model's reply into an [`Analysis`], clamping everything the card renders.
pub fn parse_analysis(text: &str) -> Result<Analysis, String> {
    let json = ai::extract_json_object(text).ok_or_else(|| no_json_error(text))?;
    let mut analysis: Analysis =
        serde_json::from_str(json).map_err(|e| format!("the model's reply did not fit the report format: {e}"))?;

    analysis.score = analysis.score.min(100);
    analysis.catalysts.truncate(MAX_BULLETS);
    analysis.risks.truncate(MAX_BULLETS);
    // A headline with no link is indistinguishable from an invented one — see the module docs
    // on `insights`. Dropping it costs one row; keeping it costs the section its credibility.
    analysis.news.retain(|item| item.url.starts_with("http"));
    analysis.news.truncate(MAX_NEWS);
    Ok(analysis)
}

pub fn parse_scan(text: &str) -> Result<ScanAnalysis, String> {
    let json = ai::extract_json_object(text).ok_or_else(|| no_json_error(text))?;
    let mut scan: ScanAnalysis =
        serde_json::from_str(json).map_err(|e| format!("the model's reply did not fit the scan format: {e}"))?;
    scan.ideas.truncate(MAX_IDEAS);
    Ok(scan)
}

/// The reply had no JSON in it at all. Its opening line goes into the message: when the model
/// refuses or asks a question instead, that sentence is the whole explanation.
fn no_json_error(text: &str) -> String {
    let head: String = text.trim().chars().take(160).collect();
    if head.is_empty() {
        "the model returned an empty reply".into()
    } else {
        format!("the model did not return a report: {head}")
    }
}

fn coin_system(language: &str) -> String {
    format!(
        "You are a crypto market analyst writing a briefing for an experienced trader.\n\
         \n\
         Use the web_search tool before answering. Search for the asset's news from the last two \
         weeks, its upcoming catalysts, and anything that changed its risk profile. Prefer \
         primary sources — exchange announcements, the project's own posts, regulator filings — \
         over aggregators.\n\
         \n\
         Then answer with a single JSON object and nothing else. No prose before it, no markdown \
         fence around it. Schema:\n\
         {{\"verdict\":\"bullish|neutral|bearish\",\"score\":0-100,\"summary\":\"2-4 sentences\",\
         \"catalysts\":[\"...\"],\"risks\":[\"...\"],\
         \"news\":[{{\"title\":\"...\",\"url\":\"https://...\",\"source\":\"...\",\
         \"published\":\"YYYY-MM-DD or a relative phrase\",\
         \"sentiment\":\"positive|neutral|negative\",\"impact\":\"one line on why it moves the price\"}}]}}\n\
         \n\
         Rules: at most 4 catalysts and 4 risks, each one concrete and specific to this asset — \
         no \"market volatility\". At most 6 news items, every one with a real URL you opened; \
         omit the item rather than guess a link. `score` is how interesting this asset is right \
         now on a comparable 0-100 scale, not a price target and not a probability. Say plainly \
         when the evidence is thin. Never give financial advice or a recommendation to buy or \
         sell. {}",
        language_clause(language)
    )
}

fn coin_user(
    asset: &str,
    symbol: &str,
    fundamentals: Option<&Fundamentals>,
    quote: Option<&crate::market::TickerSnapshot>,
) -> String {
    let mut prompt = format!(
        "Asset: {asset} (traded as {symbol}).\nToday is {}.\n",
        chrono::Utc::now().format("%Y-%m-%d")
    );

    if let Some(ticker) = quote {
        prompt.push_str(&format!(
            "Live price: {:.8} USDT, {:+.2}% over 24h, 24h quote volume {:.0} USDT.\n",
            ticker.price, ticker.percent_24h, ticker.quote_volume
        ));
    }

    if let Some(f) = fundamentals {
        prompt.push_str(&format!("CoinGecko: {} ({})", f.name, f.symbol));
        if let Some(rank) = f.market_cap_rank {
            prompt.push_str(&format!(", rank #{rank}"));
        }
        if let Some(cap) = f.market_cap_usd {
            prompt.push_str(&format!(", market cap ${cap:.0}"));
        }
        if let Some(ath) = f.ath_change_pct {
            prompt.push_str(&format!(", {ath:.1}% from its all-time high"));
        }
        if let (Some(circulating), Some(max)) = (f.circulating_supply, f.max_supply) {
            prompt.push_str(&format!(
                ", {:.1}% of the max supply in circulation",
                circulating / max * 100.0
            ));
        }
        if !f.categories.is_empty() {
            prompt.push_str(&format!(". Categories: {}", f.categories.join(", ")));
        }
        prompt.push_str(".\n");
    }

    prompt.push_str(
        "\nResearch this asset and return the JSON report. The figures above are measured — do \
         not restate them as findings; explain what is happening around them.",
    );
    prompt
}

fn scan_system(language: &str) -> String {
    format!(
        "You are a crypto market analyst surfacing projects worth a closer look.\n\
         \n\
         Use the web_search tool first: what moved this week, which narratives are gaining \
         attention, which projects have a dated catalyst ahead. Prefer primary sources over \
         aggregators, and prefer things that happened in the last two weeks over evergreen \
         theses.\n\
         \n\
         Then answer with a single JSON object and nothing else. Schema:\n\
         {{\"narrative\":\"2-3 sentences on what the market is doing right now\",\
         \"ideas\":[{{\"name\":\"Project name\",\"symbol\":\"TICKER\",\"category\":\"e.g. L2, DePIN, RWA\",\
         \"thesis\":\"1-2 sentences\",\"catalyst\":\"the specific dated or expected event\",\
         \"risk\":\"the specific thing that breaks the thesis\",\"horizon\":\"days|weeks|months\",\
         \"conviction\":0-100,\"url\":\"https://...\"}}]}}\n\
         \n\
         Rules: 4 to 6 ideas, each a different project — no two ideas on the same narrative. \
         `symbol` is the asset's real exchange ticker in capitals; leave it empty rather than \
         inventing one. Every idea needs a catalyst that is a real, checkable event, not a mood. \
         Never give financial advice or a recommendation to buy or sell. {}",
        language_clause(language)
    )
}

fn scan_user(held: &[String], trending: &[super::TrendingCoin]) -> String {
    let mut prompt = format!("Today is {}.\n", chrono::Utc::now().format("%Y-%m-%d"));

    if !held.is_empty() {
        prompt.push_str(&format!(
            "The user already watches: {}. Ideas outside that set are more useful than another \
             take on what they hold, but say so if one of these is where the action is.\n",
            held.join(", ")
        ));
    }

    if !trending.is_empty() {
        let names: Vec<String> = trending
            .iter()
            .take(10)
            .map(|c| format!("{} ({})", c.name, c.symbol))
            .collect();
        prompt.push_str(&format!(
            "CoinGecko's trending list right now: {}. That is a popularity signal, not an \
             endorsement — disagree with it where the evidence says to.\n",
            names.join(", ")
        ));
    }

    prompt.push_str("\nScan the market and return the JSON object.");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "crypto-widget-insights-{}-{}",
            std::process::id(),
            tag
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stored_insight(url: &str) -> CoinInsight {
        CoinInsight {
            symbol: "BTCUSDT".into(),
            asset: "BTC".into(),
            analysis: Analysis {
                news: vec![super::super::NewsItem {
                    title: "Something happened".into(),
                    url: url.into(),
                    source: "example.com".into(),
                    published: None,
                    sentiment: super::super::Sentiment::Neutral,
                    impact: String::new(),
                }],
                ..Analysis::default()
            },
            fundamentals: None,
            sources: Vec::new(),
            generated_at: 0,
            model: "claude-opus-5".into(),
            usage: Default::default(),
            cached: false,
        }
    }

    #[test]
    fn only_a_link_that_is_in_a_report_may_be_opened() {
        let dir = temp_cache_dir("known-url");
        crate::config::cache::write(
            &cache_path(&dir, "coin-btc"),
            &stored_insight("https://example.com/story"),
        );

        assert!(is_reported_url(&dir, "https://example.com/story"));
        // A prefix of a stored link is a different link. Matching on the file's text rather than
        // its fields would have let this through.
        assert!(!is_reported_url(&dir, "https://example.com"));
        assert!(!is_reported_url(&dir, "https://evil.example/steal"));
    }

    #[test]
    fn an_empty_cache_vouches_for_nothing() {
        let dir = temp_cache_dir("empty-cache");
        assert!(!is_reported_url(&dir, "https://example.com/story"));
    }

    #[test]
    fn a_pair_is_reduced_to_the_asset_it_prices() {
        assert_eq!(base_asset("BTCUSDT"), "BTC");
        assert_eq!(base_asset("gramusdt"), "GRAM");
        // Already an asset name: the panel can be asked about a bare ticker too.
        assert_eq!(base_asset("btc"), "BTC");
    }

    #[test]
    fn a_report_is_clamped_to_what_the_card_can_render() {
        let reply = r#"{
            "verdict":"bullish","score":250,
            "summary":"Up.",
            "catalysts":["a","b","c","d","e","f","g","h"],
            "risks":["r1","r2","r3","r4","r5","r6","r7"],
            "news":[]
        }"#;
        let analysis = parse_analysis(reply).unwrap();
        assert_eq!(analysis.score, 100, "a score above the scale is clamped, not trusted");
        assert_eq!(analysis.catalysts.len(), MAX_BULLETS);
        assert_eq!(analysis.risks.len(), MAX_BULLETS);
    }

    #[test]
    fn a_headline_without_a_link_is_dropped() {
        // The whole point of the news section is that every claim is checkable. An item with no
        // URL — or with the model's placeholder in place of one — is indistinguishable from an
        // invented headline, so it does not get to sit next to the real ones.
        let reply = r#"{
            "verdict":"neutral","score":50,
            "news":[
                {"title":"Real","url":"https://example.com/a"},
                {"title":"Made up","url":""},
                {"title":"Placeholder","url":"example.com/b"}
            ]
        }"#;
        let analysis = parse_analysis(reply).unwrap();
        assert_eq!(analysis.news.len(), 1);
        assert_eq!(analysis.news[0].title, "Real");
    }

    #[test]
    fn a_report_wrapped_in_prose_still_parses() {
        let reply = "Sure — here is the report:\n```json\n{\"verdict\":\"bearish\",\"score\":18}\n```";
        let analysis = parse_analysis(reply).unwrap();
        assert_eq!(analysis.score, 18);
    }

    #[test]
    fn a_reply_with_no_report_names_what_the_model_said_instead() {
        // The call is already billed at this point; "invalid JSON" would hide the one useful
        // thing in the response — the sentence explaining why there is no report.
        let error = parse_analysis("I can't help with investment advice.").unwrap_err();
        assert!(error.contains("investment advice"), "{error}");
        assert!(parse_analysis("").unwrap_err().contains("empty"));
    }

    #[test]
    fn a_scan_is_capped_and_its_convictions_are_kept_on_scale() {
        let reply = format!(
            r#"{{"narrative":"Rotation into L2s.","ideas":[{}]}}"#,
            (0..12)
                .map(|i| format!(r#"{{"name":"P{i}","symbol":"P{i}","conviction":200}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let scan = parse_scan(&reply).unwrap();
        assert_eq!(scan.ideas.len(), MAX_IDEAS);
        assert_eq!(scan.narrative, "Rotation into L2s.");
    }

    #[test]
    fn an_idea_arrives_with_no_binance_pair_of_its_own() {
        // `binance_symbol` is attached on this side, after a lookup. Trusting the model's own
        // field would put an unpriceable row in the watchlist.
        let scan = parse_scan(r#"{"ideas":[{"name":"X","symbol":"XYZ","binanceSymbol":"XYZUSDT"}]}"#)
            .unwrap();
        assert_eq!(scan.ideas[0].binance_symbol.as_deref(), Some("XYZUSDT"),
            "the field parses — resolve_binance_symbol overwrites it before it is stored");
    }

    #[test]
    fn the_prompt_carries_the_measured_figures_into_the_question() {
        let fundamentals = Fundamentals {
            id: "gram".into(),
            name: "Gram".into(),
            symbol: "GRAM".into(),
            market_cap_usd: Some(3_400_000_000.0),
            market_cap_rank: Some(42),
            volume24h_usd: None,
            circulating_supply: Some(2_500_000_000.0),
            max_supply: Some(5_000_000_000.0),
            ath_usd: Some(8.29),
            ath_change_pct: Some(-83.8),
            categories: vec!["Layer 1".into()],
            homepage: None,
            github: None,
            sentiment_up_pct: None,
        };
        let prompt = coin_user("GRAM", "GRAMUSDT", Some(&fundamentals), None);

        assert!(prompt.contains("rank #42"), "{prompt}");
        assert!(prompt.contains("-83.8% from its all-time high"), "{prompt}");
        assert!(prompt.contains("50.0% of the max supply"), "{prompt}");
        assert!(prompt.contains("Layer 1"), "{prompt}");
    }

    #[test]
    fn a_coin_with_no_fundamentals_still_produces_a_prompt() {
        let prompt = coin_user("NEWTOKEN", "NEWTOKENUSDT", None, None);
        assert!(prompt.contains("NEWTOKEN"));
        assert!(prompt.contains("Today is"), "the model has to know the date it is answering on");
    }

    #[test]
    fn the_language_setting_reaches_the_system_prompt() {
        assert!(coin_system("ru").contains("Russian"));
        assert!(scan_system("en").contains("English"));
        assert!(coin_system("cs").contains("'cs'"), "an unlisted code is passed through");
    }
}
