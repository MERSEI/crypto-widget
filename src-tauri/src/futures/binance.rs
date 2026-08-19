//! Signed REST client for Binance USDⓈ-M futures.
//!
//! Endpoint versioning is handled defensively. Binance runs `/fapi/v2/*` and `/fapi/v3/*` side
//! by side, migrates accounts between them, and its own docs disagree about which is current;
//! a client pinned to one version breaks for whoever is on the other. So each read tries v3 and
//! falls back to v2, and the parsers key off field names — which are stable across both — rather
//! than positions or an exact schema. Same reasoning as `market/backup.rs`: when the upstream is
//! allowed to change shape, parse what is there instead of asserting what should be.
//!
//! Every number in these payloads is a JSON *string*, and `"NaN".parse::<f64>()` succeeds — the
//! same trap that once panicked the spot ticker parser. Values go through [`field_f64`], which
//! treats a non-finite result as absent.

use super::signer::{Query, TimeSync};
use super::venue::{AccountSummary, FuturesSnapshot, FuturesVenue, Position};
use super::VenueMode;
use crate::market::now_ms;
use crate::market::ratelimit::WeightLimiter;
use crate::secrets::Credential;
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

/// How long a measured clock offset is trusted before it is taken again.
const TIME_SYNC_TTL_MS: i64 = 60 * 60 * 1000;

pub struct BinanceFutures {
    client: reqwest::Client,
    credential: Credential,
    mode: VenueMode,
    limiter: WeightLimiter,
    time: TimeSync,
    /// When the offset was last measured, in local ms. `0` = never.
    time_synced_at: std::sync::atomic::AtomicI64,
}

impl BinanceFutures {
    pub fn new(credential: Credential, mode: VenueMode) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
            credential,
            mode,
            // fapi allows 2400 weight/min. Stay well under: the widget polls, it does not trade
            // in a loop, and an IP ban costs the whole feature.
            limiter: WeightLimiter::new("futures", 600.0, 10.0),
            time: TimeSync::new(),
            time_synced_at: std::sync::atomic::AtomicI64::new(0),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.mode.base_url())
    }

    /// Measures the offset between the local clock and Binance's, at most once an hour.
    ///
    /// Without this, a machine whose clock has drifted past `recvWindow` gets `-1021` on every
    /// signed request — which reads like a broken API key, not a broken clock.
    async fn sync_time_if_stale(&self) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let last = self.time_synced_at.load(Ordering::Relaxed);
        if last != 0 && now_ms() - last < TIME_SYNC_TTL_MS {
            return Ok(());
        }

        self.limiter.take(1.0)?;
        let sent = now_ms();
        let body = self
            .client
            .get(self.url("/fapi/v1/time"))
            .send()
            .await
            .map_err(|e| format!("could not reach Binance futures: {e}"))?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let received = now_ms();

        let parsed: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let server_time = parsed
            .get("serverTime")
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("unexpected /time response: {body}"))?;

        self.time.observe(server_time, sent, received);
        self.time_synced_at.store(received, Ordering::Relaxed);
        Ok(())
    }

    /// Performs a signed GET and returns the parsed body.
    async fn signed_get(&self, path: &str, query: Query, weight: f64) -> Result<Value, String> {
        self.sync_time_if_stale().await?;
        self.limiter.take(weight)?;

        let query = query.sign_with(&self.credential.secret, self.time.now_ms())?;
        let response = self
            .client
            .get(format!("{}?{query}", self.url(path)))
            .header("X-MBX-APIKEY", &self.credential.key)
            .send()
            .await
            .map_err(|e| format!("could not reach Binance futures: {e}"))?;

        let status = response.status();
        let body = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(describe_api_error(status.as_u16(), &body));
        }
        serde_json::from_str(&body).map_err(|e| format!("unreadable response from {path}: {e}"))
    }

    /// Tries the v3 path first and falls back to v2 when the account or the endpoint is not
    /// available there. A signature or permission failure is *not* retried — it would fail
    /// identically on v2 and only doubles the noise in the error the user sees.
    async fn signed_get_versioned(&self, v3: &str, v2: &str, weight: f64) -> Result<Value, String> {
        match self.signed_get(v3, Query::new(), weight).await {
            Ok(value) => Ok(value),
            Err(e) if is_endpoint_unavailable(&e) => {
                self.signed_get(v2, Query::new(), weight).await
            }
            Err(e) => Err(e),
        }
    }
}

/// Binance answers a bad key, a wrong permission, and a drifted clock all with HTTP 401/400 and
/// a numeric code in the body. Translating the common ones saves the user a search.
fn describe_api_error(status: u16, body: &str) -> String {
    let code = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("code").and_then(Value::as_i64));
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("msg").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect());

    match code {
        Some(-1021) => "your computer's clock is out of sync with Binance — the app resyncs \
                        hourly, but Windows time may need fixing"
            .into(),
        Some(-1022) | Some(-1099) => format!("Binance rejected the signature ({message})"),
        Some(-2015) => "Binance rejected the API key: check the key, its permissions, and \
                        whether your IP is on its allow-list"
            .into(),
        Some(-2014) => "that API key is malformed".into(),
        Some(-1003) => "rate limited by Binance — back off before retrying".into(),
        Some(code) => format!("Binance error {code}: {message}"),
        None => format!("Binance returned HTTP {status}: {message}"),
    }
}

/// True for the "this path/version isn't there for you" family, which is what makes a v2 retry
/// worth doing.
fn is_endpoint_unavailable(error: &str) -> bool {
    error.contains("HTTP 404") || error.contains("error -1100") || error.contains("error -1121")
}

/// Reads a stringified (or numeric) field, rejecting the values that parse but are not numbers.
fn field_f64(value: &Value, key: &str) -> Option<f64> {
    let raw = value.get(key)?;
    let parsed = match raw {
        Value::String(s) => s.parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    parsed.is_finite().then_some(parsed)
}

fn field_str(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

/// A liquidation price of zero means "not applicable", not "liquidates at zero".
fn liquidation_price(value: &Value) -> Option<f64> {
    field_f64(value, "liquidationPrice").filter(|p| *p > 0.0)
}

pub fn parse_positions(value: &Value) -> Vec<Position> {
    let rows = match value {
        // `/positionRisk` answers with an array; `/account` wraps it in `positions`.
        Value::Array(rows) => rows.clone(),
        Value::Object(_) => value
            .get("positions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    rows.iter()
        .filter_map(|row| {
            let position_amt = field_f64(row, "positionAmt")?;
            Some(Position {
                symbol: field_str(row, "symbol")?,
                position_side: field_str(row, "positionSide").unwrap_or_else(|| "BOTH".into()),
                position_amt,
                entry_price: field_f64(row, "entryPrice").unwrap_or(0.0),
                mark_price: field_f64(row, "markPrice").unwrap_or(0.0),
                unrealized_pnl: field_f64(row, "unRealizedProfit")
                    .or_else(|| field_f64(row, "unrealizedProfit"))
                    .unwrap_or(0.0),
                leverage: field_f64(row, "leverage").map(|l| l as u32),
                liquidation_price: liquidation_price(row),
                margin_type: field_str(row, "marginType")
                    .or_else(|| {
                        // v3 reports a boolean `isolated` instead of a `marginType` string.
                        row.get("isolated")
                            .and_then(Value::as_bool)
                            .map(|isolated| if isolated { "isolated".into() } else { "cross".into() })
                    })
                    .unwrap_or_else(|| "cross".into()),
                notional: field_f64(row, "notional")
                    .map(f64::abs)
                    .unwrap_or_else(|| {
                        (position_amt * field_f64(row, "markPrice").unwrap_or(0.0)).abs()
                    }),
                price_change_percent: 0.0,
                roe_percent: None,
            }
            .with_derived())
        })
        .filter(Position::is_open)
        .collect()
}

pub fn parse_account(value: &Value) -> AccountSummary {
    AccountSummary {
        total_wallet_balance: field_f64(value, "totalWalletBalance").unwrap_or(0.0),
        total_unrealized_pnl: field_f64(value, "totalUnrealizedProfit").unwrap_or(0.0),
        total_margin_balance: field_f64(value, "totalMarginBalance").unwrap_or(0.0),
        available_balance: field_f64(value, "availableBalance").unwrap_or(0.0),
    }
}

#[async_trait]
impl FuturesVenue for BinanceFutures {
    async fn snapshot(&self) -> Result<FuturesSnapshot, String> {
        let account_body = self
            .signed_get_versioned("/fapi/v3/account", "/fapi/v2/account", 5.0)
            .await?;
        let account = parse_account(&account_body);

        // `/account` already carries positions, but without `markPrice` on some versions —
        // and a position table without a mark price shows no PnL movement at all. Ask
        // `positionRisk` for the authoritative rows, and fall back to the account payload if
        // that call is the one that is unavailable.
        let positions = match self
            .signed_get_versioned("/fapi/v3/positionRisk", "/fapi/v2/positionRisk", 5.0)
            .await
        {
            Ok(rows) => parse_positions(&rows),
            Err(e) if is_endpoint_unavailable(&e) => parse_positions(&account_body),
            Err(e) => return Err(e),
        };

        Ok(FuturesSnapshot {
            account,
            positions,
            updated_at: now_ms(),
        })
    }

    async fn check_credentials(&self) -> Result<String, String> {
        let body = self
            .signed_get_versioned("/fapi/v3/balance", "/fapi/v2/balance", 5.0)
            .await?;
        let assets = body.as_array().map(Vec::len).unwrap_or(0);
        let venue = match self.mode {
            VenueMode::Mainnet => "mainnet",
            VenueMode::Testnet => "testnet",
        };
        let drift = self.time.offset_ms();
        Ok(format!(
            "Connected to Binance futures {venue} — {assets} assets, clock offset {drift} ms"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a real `/fapi/v2/positionRisk` response: one open long, one flat row that
    /// the account has simply touched before.
    const POSITION_RISK_V2: &str = r#"[
        {"symbol":"BTCUSDT","positionAmt":"0.012","entryPrice":"64210.5","markPrice":"64880.20000000",
         "unRealizedProfit":"8.03640000","liquidationPrice":"58340.12","leverage":"10",
         "marginType":"cross","positionSide":"BOTH","notional":"778.5624"},
        {"symbol":"ETHUSDT","positionAmt":"0.000","entryPrice":"0.0","markPrice":"3120.10",
         "unRealizedProfit":"0","liquidationPrice":"0","leverage":"20","marginType":"cross",
         "positionSide":"BOTH","notional":"0"}
    ]"#;

    fn parse(json: &str) -> Vec<Position> {
        parse_positions(&serde_json::from_str(json).unwrap())
    }

    #[test]
    fn parses_a_real_position_risk_payload() {
        let positions = parse(POSITION_RISK_V2);
        assert_eq!(positions.len(), 1, "the flat ETH row must not be listed");

        let btc = &positions[0];
        assert_eq!(btc.symbol, "BTCUSDT");
        assert!((btc.position_amt - 0.012).abs() < 1e-12);
        assert!((btc.mark_price - 64_880.2).abs() < 1e-9);
        assert!((btc.unrealized_pnl - 8.0364).abs() < 1e-9);
        assert_eq!(btc.leverage, Some(10));
        assert_eq!(btc.liquidation_price, Some(58_340.12));
        assert!(btc.is_long());
    }

    /// The renderer reads ROE and the price move straight off the payload, so they have to be
    /// filled in by the parser — not left at the struct's zero values.
    #[test]
    fn parsed_positions_carry_their_derived_figures() {
        let btc = &parse(POSITION_RISK_V2)[0];
        assert!((btc.price_change_percent - 1.043).abs() < 1e-3, "{}", btc.price_change_percent);
        let roe = btc.roe_percent.expect("leverage was reported, so ROE is known");
        assert!((roe - 10.32).abs() < 0.05, "{roe}");
    }

    #[test]
    fn parses_the_v3_shape_with_isolated_as_a_boolean() {
        let positions = parse(
            r#"[{"symbol":"BTCUSDT","positionAmt":"-0.5","entryPrice":"64000","markPrice":"63000",
                 "unrealizedProfit":"500","liquidationPrice":"71000","isolated":true,
                 "positionSide":"SHORT","notional":"-31500"}]"#,
        );
        let short = &positions[0];
        assert_eq!(short.margin_type, "isolated");
        assert!(!short.is_long());
        // `unrealizedProfit` spelled without the capital R is read too.
        assert!((short.unrealized_pnl - 500.0).abs() < 1e-9);
        // A negative notional is reported as an absolute position value.
        assert!((short.notional - 31_500.0).abs() < 1e-9);
        assert!(short.leverage.is_none(), "v3 rows may omit leverage entirely");
    }

    #[test]
    fn reads_positions_out_of_an_account_payload_too() {
        let positions = parse(
            r#"{"totalWalletBalance":"480.0","positions":[
                {"symbol":"SOLUSDT","positionAmt":"3","entryPrice":"140","markPrice":"142",
                 "unRealizedProfit":"6","leverage":"5","positionSide":"BOTH"}]}"#,
        );
        assert_eq!(positions.len(), 1);
        // No `notional` field: it is derived from amount × mark.
        assert!((positions[0].notional - 426.0).abs() < 1e-9);
    }

    #[test]
    fn a_liquidation_price_of_zero_is_reported_as_none() {
        let positions = parse(
            r#"[{"symbol":"BTCUSDT","positionAmt":"1","entryPrice":"100","markPrice":"100",
                 "unRealizedProfit":"0","liquidationPrice":"0","leverage":"1"}]"#,
        );
        assert_eq!(positions[0].liquidation_price, None);
    }

    #[test]
    fn nan_and_junk_never_reach_a_price_field() {
        // `"NaN".parse::<f64>()` succeeds — this is the trap that panicked the spot parser.
        let positions = parse(
            r#"[{"symbol":"BTCUSDT","positionAmt":"0.5","entryPrice":"NaN","markPrice":"inf",
                 "unRealizedProfit":"oops","liquidationPrice":"NaN","leverage":"10"}]"#,
        );
        let p = &positions[0];
        assert_eq!(p.entry_price, 0.0);
        assert_eq!(p.mark_price, 0.0);
        assert_eq!(p.unrealized_pnl, 0.0);
        assert_eq!(p.liquidation_price, None);
        assert!(p.price_change_percent.is_finite());
    }

    #[test]
    fn a_row_without_a_symbol_is_skipped_rather_than_defaulted() {
        let positions = parse(r#"[{"positionAmt":"1","markPrice":"10"}]"#);
        assert!(positions.is_empty());
    }

    #[test]
    fn an_unexpected_body_yields_no_positions_instead_of_panicking() {
        assert!(parse("null").is_empty());
        assert!(parse(r#""rate limited""#).is_empty());
        assert!(parse("{}").is_empty());
    }

    #[test]
    fn parses_account_totals() {
        let account = parse_account(
            &serde_json::from_str(
                r#"{"totalWalletBalance":"480.00000000","totalUnrealizedProfit":"8.03640000",
                    "totalMarginBalance":"488.03640000","availableBalance":"420.15"}"#,
            )
            .unwrap(),
        );
        assert!((account.total_wallet_balance - 480.0).abs() < 1e-9);
        assert!((account.total_margin_balance - 488.0364).abs() < 1e-9);
        assert!((account.available_balance - 420.15).abs() < 1e-9);
    }

    #[test]
    fn missing_account_fields_read_as_zero_not_as_an_error() {
        let account = parse_account(&serde_json::from_str(r#"{"totalWalletBalance":"100"}"#).unwrap());
        assert!((account.total_wallet_balance - 100.0).abs() < 1e-9);
        assert_eq!(account.available_balance, 0.0);
    }

    #[test]
    fn the_common_api_errors_are_explained_rather_than_echoed() {
        let clock = describe_api_error(400, r#"{"code":-1021,"msg":"Timestamp for this request..."}"#);
        assert!(clock.contains("clock"), "{clock}");

        let key = describe_api_error(401, r#"{"code":-2015,"msg":"Invalid API-key, IP, or permissions"}"#);
        assert!(key.contains("allow-list"), "{key}");

        // An unrecognised code still surfaces the exchange's own message.
        let other = describe_api_error(400, r#"{"code":-4131,"msg":"Counterparty best price far away"}"#);
        assert!(other.contains("-4131") && other.contains("Counterparty"), "{other}");

        // A non-JSON body must not be swallowed.
        let html = describe_api_error(502, "<html>bad gateway</html>");
        assert!(html.contains("502"), "{html}");
    }

    #[test]
    fn only_missing_endpoint_errors_trigger_the_v2_fallback() {
        assert!(is_endpoint_unavailable("Binance returned HTTP 404: not found"));
        assert!(is_endpoint_unavailable("Binance error -1100: illegal characters"));
        assert!(!is_endpoint_unavailable(
            "Binance rejected the API key: check the key, its permissions"
        ));
        assert!(!is_endpoint_unavailable("could not reach Binance futures: timeout"));
    }
}
