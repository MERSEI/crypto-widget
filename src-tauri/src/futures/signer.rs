//! Request signing for Binance's SIGNED endpoints.
//!
//! Two things break signed requests in ways that look like nothing else:
//!
//! - **The signature covers the exact query string that is sent.** Re-encode a value between
//!   signing and sending, or reorder the parameters, and the server answers "Signature for this
//!   request is not valid" with no hint as to which half is wrong. So the query is built once,
//!   as a string, and both signed and sent verbatim. Values are validated rather than escaped:
//!   everything this app sends is an ASCII symbol, number, or enum, and a value that needs
//!   escaping is a bug, not a case to handle.
//! - **The clock has to agree with Binance's.** A Windows machine that has drifted a couple of
//!   seconds gets every request rejected with `-1021 Timestamp for this request was 1000ms ahead
//!   of the server's time`. [`TimeSync`] holds the measured offset so timestamps are stamped in
//!   server time, not local time.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::atomic::{AtomicI64, Ordering};

/// Binance rejects a timestamp older than this many milliseconds. 5s is the API default.
pub const RECV_WINDOW_MS: u64 = 5_000;

/// HMAC-SHA256 of `query` under `secret`, hex-encoded — the `signature` parameter.
pub fn sign(secret: &str, query: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(query.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Ordered query-string builder.
///
/// Parameter order is preserved because the signature is computed over the rendered string:
/// whatever order the caller adds them in is the order that gets signed and sent.
#[derive(Debug, Default, Clone)]
pub struct Query {
    parts: Vec<(String, String)>,
}

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a parameter, rejecting anything that would need percent-encoding.
    pub fn push(&mut self, key: &str, value: impl ToString) -> Result<(), String> {
        let value = value.to_string();
        if !value.chars().all(is_query_safe) {
            return Err(format!("unsafe value for query parameter `{key}`: {value:?}"));
        }
        self.parts.push((key.to_string(), value));
        Ok(())
    }

    pub fn render(&self) -> String {
        self.parts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Finishes a SIGNED request: appends `recvWindow` and `timestamp`, then the signature over
    /// everything before it. The signature is always last, as Binance's examples show.
    pub fn sign_with(mut self, secret: &str, timestamp_ms: i64) -> Result<String, String> {
        self.push("recvWindow", RECV_WINDOW_MS)?;
        self.push("timestamp", timestamp_ms)?;
        let body = self.render();
        let signature = sign(secret, &body);
        Ok(format!("{body}&signature={signature}"))
    }
}

/// Characters that survive a round-trip through a query string untouched.
fn is_query_safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')
}

/// The app's estimate of Binance's clock.
///
/// Stored as an offset rather than a synced clock so a failed sync degrades to "use local
/// time" instead of freezing the last known server time.
#[derive(Debug, Default)]
pub struct TimeSync {
    offset_ms: AtomicI64,
}

impl TimeSync {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an offset from a `/fapi/v1/time` round-trip.
    ///
    /// `local_sent` and `local_received` bracket the request, so half the round-trip is
    /// discounted: without that, every offset would be skewed by the latency of measuring it.
    pub fn observe(&self, server_time_ms: i64, local_sent_ms: i64, local_received_ms: i64) {
        let round_trip = (local_received_ms - local_sent_ms).max(0);
        let local_at_server_time = local_sent_ms + round_trip / 2;
        self.offset_ms
            .store(server_time_ms - local_at_server_time, Ordering::Relaxed);
    }

    pub fn offset_ms(&self) -> i64 {
        self.offset_ms.load(Ordering::Relaxed)
    }

    /// Current time in Binance's frame of reference.
    pub fn now_ms(&self) -> i64 {
        crate::market::now_ms() + self.offset_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example from Binance's "Endpoint security type" documentation. If this ever
    /// fails, the signing itself is wrong — not the endpoint, not the clock.
    #[test]
    fn matches_the_documented_signature_vector() {
        let secret = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j";
        let query = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";
        assert_eq!(
            sign(secret, query),
            "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71"
        );
    }

    /// Builds a query from an ordered parameter list, the way callers do.
    fn query(pairs: &[(&str, &str)]) -> Query {
        let mut query = Query::new();
        for (key, value) in pairs {
            query.push(key, value).expect("test values are query-safe");
        }
        query
    }

    #[test]
    fn the_query_keeps_the_order_it_was_built_in() {
        let query = query(&[("symbol", "BTCUSDT"), ("side", "BUY"), ("quantity", "0.012")]);
        assert_eq!(query.render(), "symbol=BTCUSDT&side=BUY&quantity=0.012");
    }

    #[test]
    fn signing_appends_recv_window_timestamp_and_signature_in_that_order() {
        let signed = query(&[("symbol", "BTCUSDT")])
            .sign_with("secret", 1_499_827_319_559)
            .unwrap();
        assert!(
            signed.starts_with("symbol=BTCUSDT&recvWindow=5000&timestamp=1499827319559&signature="),
            "{signed}"
        );
        // The signature covers everything before it, and nothing else.
        let (body, sig) = signed.rsplit_once("&signature=").unwrap();
        assert_eq!(sign("secret", body), sig);
    }

    #[test]
    fn a_value_that_would_need_escaping_is_refused() {
        let mut query = Query::new();
        assert!(query.push("symbol", "BTC/USDT").is_err());
        assert!(query.push("note", "a b").is_err());
        assert!(query.push("evil", "x&side=SELL").is_err());
        // ...and the safe shapes this app actually sends all pass.
        assert!(query.push("symbol", "BTCUSDT").is_ok());
        assert!(query.push("quantity", 0.0012_f64).is_ok());
        assert!(query.push("leverage", 10_u32).is_ok());
        assert!(query.push("clientId", "cw-1a2b_3c.4d~5e").is_ok());
    }

    #[test]
    fn a_rejected_value_does_not_land_in_the_query() {
        let mut query = Query::new();
        query.push("symbol", "BTCUSDT").unwrap();
        let _ = query.push("evil", "x&side=SELL");
        assert_eq!(query.render(), "symbol=BTCUSDT");
    }

    #[test]
    fn the_offset_discounts_half_the_round_trip() {
        let sync = TimeSync::new();
        // Request sent at 1000, answered at 1200; the server said 5100. Its clock therefore
        // read 5100 when ours read 1100 — an offset of +4000, not +4100.
        sync.observe(5_100, 1_000, 1_200);
        assert_eq!(sync.offset_ms(), 4_000);
    }

    #[test]
    fn a_fresh_sync_defaults_to_local_time() {
        let sync = TimeSync::new();
        assert_eq!(sync.offset_ms(), 0);
        let drift = (sync.now_ms() - crate::market::now_ms()).abs();
        assert!(drift < 1_000, "unsynced time must track the local clock");
    }

    #[test]
    fn a_backwards_round_trip_does_not_corrupt_the_offset() {
        // Monotonicity is not guaranteed across a clock adjustment mid-request.
        let sync = TimeSync::new();
        sync.observe(5_000, 1_200, 1_000);
        assert_eq!(sync.offset_ms(), 3_800, "the negative round trip is clamped to zero");
    }
}
