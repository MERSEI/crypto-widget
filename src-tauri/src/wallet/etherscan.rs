//! Transaction history, via the Etherscan v2 API.
//!
//! Three things this client is built to get right, each of them a bug the previous
//! implementation of this feature shipped with:
//!
//! - **v2 only.** The v1 endpoints (`api.etherscan.io/api?module=…`) answer every request with
//!   `NOTOK / "You are using a deprecated V1 endpoint"`. v2 is one host for every chain,
//!   selected by `chainid`.
//! - **A failure is never flattened into an empty list.** `status: "0"` means either "no
//!   transactions found" — which is not an error — or a real failure, and the two are only
//!   distinguishable by the message. Returning `[]` for both made a rejected API key look
//!   exactly like a brand-new wallet.
//! - **`to` is null for contract creations.** Unconditionally reading it is what took down the
//!   whole history render the moment such a transaction appeared.

use serde::{Deserialize, Serialize};
use std::time::Duration;

const BASE: &str = "https://api.etherscan.io/v2/api";
const TIMEOUT: Duration = Duration::from_secs(12);

/// Credential-store namespace for the API key. The key is a quota, not money — but the app
/// already has one place for secrets, and settings.json is explicitly not it.
pub const NAMESPACE: &str = "etherscan";

/// One row as Etherscan returns it. Every field is optional because the two endpoints
/// (`txlist`, `tokentx`) return different subsets and a missing field must not fail the parse.
#[derive(Debug, Clone, Deserialize)]
struct RawTx {
    hash: Option<String>,
    from: Option<String>,
    /// `None` (or empty) for contract-creation transactions.
    to: Option<String>,
    value: Option<String>,
    #[serde(rename = "timeStamp")]
    time_stamp: Option<String>,
    #[serde(rename = "tokenDecimal")]
    token_decimal: Option<String>,
    #[serde(rename = "tokenSymbol")]
    token_symbol: Option<String>,
    #[serde(rename = "contractAddress")]
    contract_address: Option<String>,
    #[serde(rename = "isError")]
    is_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Native,
    Token,
}

/// A normalised history row, ready for the renderer. Amounts are strings for the same reason
/// as in `chain.rs`: a JS number cannot hold them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub hash: String,
    pub from: String,
    /// `None` for a contract creation.
    pub to: Option<String>,
    pub amount: String,
    /// Unix milliseconds.
    pub timestamp: i64,
    pub direction: Direction,
    pub asset: AssetKind,
    pub symbol: String,
    /// True when the transaction reverted. Kept in the list — it happened — but it moved no
    /// value, so nothing derived from the totals may count it.
    pub failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Config,
    RateLimit,
    Upstream,
    Network,
}

#[derive(Debug, Clone)]
pub struct EtherscanError {
    pub kind: ErrorKind,
    pub message: String,
}

impl std::fmt::Display for EtherscanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl EtherscanError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Envelope {
    status: Option<String>,
    message: Option<String>,
    result: Option<serde_json::Value>,
}

/// Decides what an Etherscan envelope actually means.
///
/// Split out from the request so every branch is testable without a network — which is the
/// only way to be sure "no transactions found" and "your key was rejected" stay distinguishable.
fn interpret(body: Envelope) -> Result<Vec<RawTx>, EtherscanError> {
    if body.status.as_deref() == Some("1") {
        if let Some(value) = body.result {
            return serde_json::from_value(value)
                .map_err(|e| EtherscanError::new(ErrorKind::Upstream, format!("unreadable response: {e}")));
        }
    }

    let message = body.message.unwrap_or_default();
    let detail = match body.result {
        Some(serde_json::Value::String(s)) => s,
        // A successful-looking envelope whose result is an array but whose status is not "1":
        // treat the rows as authoritative, since that is what they are.
        Some(value @ serde_json::Value::Array(_)) => {
            return serde_json::from_value(value)
                .map_err(|e| EtherscanError::new(ErrorKind::Upstream, format!("unreadable response: {e}")));
        }
        _ => String::new(),
    };

    let combined = format!("{message} {detail}").to_lowercase();

    if combined.contains("no transactions found") || combined.contains("no records found") {
        return Ok(Vec::new());
    }
    if combined.contains("rate limit") || combined.contains("max calls per sec") {
        return Err(EtherscanError::new(
            ErrorKind::RateLimit,
            "Etherscan rate limit reached — try again in a moment",
        ));
    }
    if combined.contains("invalid api key") || combined.contains("missing") && combined.contains("api key") {
        return Err(EtherscanError::new(
            ErrorKind::Config,
            "Etherscan rejected the API key",
        ));
    }
    if combined.contains("deprecated v1 endpoint") {
        return Err(EtherscanError::new(
            ErrorKind::Config,
            "Etherscan v1 is deprecated; this build must use v2",
        ));
    }

    Err(EtherscanError::new(
        ErrorKind::Upstream,
        if detail.is_empty() { message } else { detail },
    ))
}

fn build_url(chain_id: u64, params: &[(&str, String)], api_key: &str) -> Result<String, EtherscanError> {
    let mut url = url::Url::parse(BASE)
        .map_err(|e| EtherscanError::new(ErrorKind::Config, e.to_string()))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("chainid", &chain_id.to_string());
        for (key, value) in params {
            query.append_pair(key, value);
        }
        query.append_pair("apikey", api_key);
    }
    Ok(url.to_string())
}

async fn call(
    client: &reqwest::Client,
    chain_id: u64,
    api_key: &str,
    params: &[(&str, String)],
) -> Result<Vec<RawTx>, EtherscanError> {
    if api_key.trim().is_empty() {
        return Err(EtherscanError::new(
            ErrorKind::Config,
            "no Etherscan API key is configured",
        ));
    }

    let url = build_url(chain_id, params, api_key)?;

    let response = client
        .get(url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| EtherscanError::new(ErrorKind::Network, format!("Etherscan unreachable: {e}")))?;

    if response.status().as_u16() == 429 {
        return Err(EtherscanError::new(
            ErrorKind::RateLimit,
            "Etherscan rate limit reached — try again in a moment",
        ));
    }
    if !response.status().is_success() {
        return Err(EtherscanError::new(
            ErrorKind::Upstream,
            format!("Etherscan returned {}", response.status().as_u16()),
        ));
    }

    let body: Envelope = response
        .json()
        .await
        .map_err(|_| EtherscanError::new(ErrorKind::Upstream, "Etherscan returned a malformed response"))?;

    interpret(body)
}

/// Normalises a raw row into a [`Transfer`], or `None` when it is too broken to show.
fn normalize(raw: &RawTx, owner: &str, asset: AssetKind, fallback_symbol: &str) -> Option<Transfer> {
    let hash = raw.hash.clone().filter(|h| !h.is_empty())?;

    let to = raw
        .to
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string());
    let from = raw.from.clone().unwrap_or_default();

    let decimals: u8 = match asset {
        AssetKind::Native => 18,
        AssetKind::Token => raw
            .token_decimal
            .as_deref()
            .and_then(|d| d.parse().ok())
            .unwrap_or(18),
    };

    let amount = raw
        .value
        .as_deref()
        .and_then(|v| v.parse::<alloy::primitives::U256>().ok())
        .map(|v| super::chain::format_amount(v, decimals))
        .unwrap_or_else(|| "0".to_string());

    let timestamp = raw
        .time_stamp
        .as_deref()
        .and_then(|t| t.parse::<i64>().ok())
        .map(|seconds| seconds * 1000)
        .unwrap_or(0);

    // A transfer *to* the owner is incoming; everything else — including a contract creation,
    // whose `to` is null — is outgoing.
    let direction = match to.as_deref() {
        Some(t) if t.eq_ignore_ascii_case(owner) => Direction::In,
        _ => Direction::Out,
    };

    let symbol = match asset {
        AssetKind::Native => "ETH".to_string(),
        AssetKind::Token => raw
            .token_symbol
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_symbol.to_string()),
    };

    Some(Transfer {
        hash,
        from,
        to,
        amount,
        timestamp,
        direction,
        asset,
        symbol,
        failed: raw.is_error.as_deref() == Some("1"),
    })
}

/// Native-currency and ERC-20 history for an address, newest first.
///
/// The two endpoints are requested in sequence rather than concurrently: the free Etherscan
/// tier allows 5 calls a second across the whole key, and two parallel bursts from a wallet
/// that also refreshes on a timer is how a key starts answering nothing but rate-limit errors.
pub async fn fetch_history(
    client: &reqwest::Client,
    chain_id: u64,
    api_key: &str,
    address: &str,
    token_contracts: &[String],
    limit: u32,
) -> Result<Vec<Transfer>, EtherscanError> {
    let common = |action: &str| {
        vec![
            ("module", "account".to_string()),
            ("action", action.to_string()),
            ("address", address.to_string()),
            ("startblock", "0".to_string()),
            ("endblock", "99999999".to_string()),
            ("page", "1".to_string()),
            ("offset", limit.to_string()),
            ("sort", "desc".to_string()),
        ]
    };

    let mut transfers: Vec<Transfer> = Vec::new();

    let native = call(client, chain_id, api_key, &common("txlist")).await?;
    transfers.extend(
        native
            .iter()
            .filter_map(|raw| normalize(raw, address, AssetKind::Native, "ETH")),
    );

    // One `tokentx` call without a contract filter returns transfers of *every* token the
    // address has touched, which is what a wallet showing several tokens actually wants — and
    // it costs one request instead of one per token.
    let tokens = call(client, chain_id, api_key, &common("tokentx")).await?;
    let interesting = |raw: &RawTx| {
        token_contracts.is_empty()
            || raw
                .contract_address
                .as_deref()
                .is_some_and(|c| token_contracts.iter().any(|t| t.eq_ignore_ascii_case(c)))
    };
    transfers.extend(
        tokens
            .iter()
            .filter(|raw| interesting(raw))
            .filter_map(|raw| normalize(raw, address, AssetKind::Token, "TOKEN")),
    );

    // Newest first, and by descending timestamp rather than by endpoint: the two calls each
    // return their own sorted run, and concatenating them would interleave badly.
    transfers.sort_by_key(|t| std::cmp::Reverse(t.timestamp));
    Ok(transfers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(json: &str) -> Envelope {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn an_empty_history_is_not_an_error() {
        let rows = interpret(envelope(
            r#"{"status":"0","message":"No transactions found","result":[]}"#,
        ))
        .unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn a_rejected_key_never_looks_like_an_empty_wallet() {
        // The bug this pins down: both cases arrive as status "0", and flattening them into
        // `[]` told the user their funded address had no history at all.
        let err = interpret(envelope(
            r#"{"status":"0","message":"NOTOK","result":"Invalid API Key"}"#,
        ))
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Config);
    }

    #[test]
    fn a_deprecated_v1_response_is_reported_as_configuration() {
        let err = interpret(envelope(
            r#"{"status":"0","message":"NOTOK","result":"You are using a deprecated V1 endpoint"}"#,
        ))
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Config);
    }

    #[test]
    fn a_rate_limit_is_told_apart_from_a_bad_key() {
        let err = interpret(envelope(
            r#"{"status":"0","message":"NOTOK","result":"Max calls per sec rate limit reached"}"#,
        ))
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::RateLimit);
    }

    #[test]
    fn rows_are_parsed_when_the_status_says_so() {
        let rows = interpret(envelope(
            r#"{"status":"1","message":"OK","result":[{"hash":"0xabc","from":"0x1","to":"0x2","value":"1000","timeStamp":"1700000000"}]}"#,
        ))
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].hash.as_deref(), Some("0xabc"));
    }

    #[test]
    fn a_contract_creation_does_not_take_down_the_whole_history() {
        // `to` is null here. Reading it unconditionally is what used to throw.
        let raw = RawTx {
            hash: Some("0xdeadbeef".into()),
            from: Some("0xowner".into()),
            to: None,
            value: Some("0".into()),
            time_stamp: Some("1700000000".into()),
            token_decimal: None,
            token_symbol: None,
            contract_address: None,
            is_error: None,
        };
        let transfer = normalize(&raw, "0xowner", AssetKind::Native, "ETH").unwrap();
        assert_eq!(transfer.to, None);
        assert_eq!(transfer.direction, Direction::Out);
    }

    #[test]
    fn direction_is_decided_case_insensitively() {
        // Etherscan returns lowercase addresses; the wallet holds checksummed ones. Comparing
        // them literally would mark every incoming transfer as outgoing.
        let raw = RawTx {
            hash: Some("0x1".into()),
            from: Some("0xsender".into()),
            to: Some("0x9858effd232b4033e47d90003d41ec34ecaeda94".into()),
            value: Some("1000000000000000000".into()),
            time_stamp: Some("1700000000".into()),
            token_decimal: None,
            token_symbol: None,
            contract_address: None,
            is_error: None,
        };
        let transfer = normalize(&raw, "0x9858EfFD232B4033E47d90003D41EC34EcaEda94", AssetKind::Native, "ETH")
            .unwrap();
        assert_eq!(transfer.direction, Direction::In);
        assert_eq!(transfer.amount, "1.000000000000000000");
    }

    #[test]
    fn token_amounts_use_the_decimals_the_row_reports() {
        let raw = RawTx {
            hash: Some("0x1".into()),
            from: Some("0xsender".into()),
            to: Some("0xowner".into()),
            value: Some("2500000".into()),
            time_stamp: Some("1700000000".into()),
            token_decimal: Some("6".into()),
            token_symbol: Some("USDC".into()),
            contract_address: Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into()),
            is_error: None,
        };
        let transfer = normalize(&raw, "0xowner", AssetKind::Token, "TOKEN").unwrap();
        assert_eq!(transfer.amount, "2.500000");
        assert_eq!(transfer.symbol, "USDC");
    }

    #[test]
    fn a_reverted_transaction_is_kept_but_marked() {
        let raw = RawTx {
            hash: Some("0x1".into()),
            from: Some("0xowner".into()),
            to: Some("0xelse".into()),
            value: Some("1000".into()),
            time_stamp: Some("1700000000".into()),
            token_decimal: None,
            token_symbol: None,
            contract_address: None,
            is_error: Some("1".into()),
        };
        assert!(normalize(&raw, "0xowner", AssetKind::Native, "ETH").unwrap().failed);
    }

    #[test]
    fn a_row_without_a_hash_is_dropped() {
        let raw = RawTx {
            hash: None,
            from: Some("0xowner".into()),
            to: Some("0xelse".into()),
            value: Some("1".into()),
            time_stamp: Some("1".into()),
            token_decimal: None,
            token_symbol: None,
            contract_address: None,
            is_error: None,
        };
        assert!(normalize(&raw, "0xowner", AssetKind::Native, "ETH").is_none());
    }

    #[test]
    fn the_url_is_v2_and_carries_the_chain_id() {
        let url = build_url(1, &[("module", "account".into())], "KEY").unwrap();
        assert!(url.starts_with("https://api.etherscan.io/v2/api?"), "{url}");
        assert!(url.contains("chainid=1"), "{url}");
        assert!(url.contains("apikey=KEY"), "{url}");
    }
}
