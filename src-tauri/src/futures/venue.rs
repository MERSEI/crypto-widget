//! The shape of a futures account, and the interface any venue has to satisfy.
//!
//! Only Binance implements this today. The trait exists anyway because the numbers below are
//! what the UI is written against: adding Bybit later should mean writing one more impl, not
//! reshaping the panel.

use async_trait::async_trait;
use serde::Serialize;

/// One open position.
///
/// `position_amt` is signed — negative is short — which is how Binance reports it and how the
/// side is derived in one-way mode, where `position_side` is always `BOTH`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub symbol: String,
    /// `LONG`, `SHORT`, or `BOTH` (one-way mode).
    pub position_side: String,
    pub position_amt: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    /// Absent on venues (and endpoint versions) that stopped reporting it per position.
    pub leverage: Option<u32>,
    /// `None` when the position cannot be liquidated at any price the API will name — an
    /// unleveraged cross position reports `0`, which is not a price and must not be shown as one.
    pub liquidation_price: Option<f64>,
    /// `cross` or `isolated`.
    pub margin_type: String,
    /// Position value in quote currency, absolute.
    pub notional: f64,
    /// Derived by [`Position::with_derived`] — see there for why these are stored rather than
    /// recomputed in the renderer.
    pub price_change_percent: f64,
    pub roe_percent: Option<f64>,
}

impl Position {
    /// Fills the derived fields from the reported ones.
    ///
    /// They are carried in the struct rather than left as methods because the renderer needs
    /// them: a `#[tauri::command]` serializes fields, not behaviour, and reimplementing ROE in
    /// TypeScript would put the one formula a trader reads first in two places, tested in one.
    pub fn with_derived(mut self) -> Self {
        self.price_change_percent = self.compute_price_change_percent();
        self.roe_percent = self.compute_roe_percent();
        self
    }
    /// True for a position that exists only as a zero row. Binance returns one for every symbol
    /// the account has ever touched, and rendering those would bury the real ones.
    pub fn is_open(&self) -> bool {
        self.position_amt != 0.0
    }

    pub fn is_long(&self) -> bool {
        self.position_amt > 0.0
    }

    /// How far the mark has moved from the entry, in percent, signed by the direction of the
    /// position. Independent of leverage.
    fn compute_price_change_percent(&self) -> f64 {
        if self.entry_price == 0.0 {
            return 0.0;
        }
        let direction = if self.is_long() { 1.0 } else { -1.0 };
        (self.mark_price - self.entry_price) / self.entry_price * 100.0 * direction
    }

    /// Return on the margin actually committed — the number a trader reads first.
    ///
    /// Derived from the reported PnL and margin rather than from leverage × price move: the
    /// two disagree once a position has been added to at a different price, and the reported
    /// figures are the ones that match the exchange's own screen.
    fn compute_roe_percent(&self) -> Option<f64> {
        let leverage = self.leverage? as f64;
        if leverage <= 0.0 || self.notional == 0.0 {
            return None;
        }
        let margin = self.notional / leverage;
        Some(self.unrealized_pnl / margin * 100.0)
    }
}

/// Account-level totals, in the account's quote asset (USDT for USDⓈ-M).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub total_wallet_balance: f64,
    pub total_unrealized_pnl: f64,
    pub total_margin_balance: f64,
    pub available_balance: f64,
}

/// Everything the panel renders in one object, so a partial refresh can never show a balance
/// from one moment next to positions from another.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesSnapshot {
    pub account: AccountSummary,
    pub positions: Vec<Position>,
    /// Unix ms, local clock — when this snapshot was assembled.
    pub updated_at: i64,
}

#[async_trait]
pub trait FuturesVenue: Send + Sync {
    /// Balances and open positions, as of now.
    async fn snapshot(&self) -> Result<FuturesSnapshot, String>;

    /// Cheap authenticated call used to check that a key works and has the right permissions.
    /// Returns a human-readable summary for the "Test connection" button.
    async fn check_credentials(&self) -> Result<String, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(amt: f64, entry: f64, mark: f64, pnl: f64, leverage: Option<u32>) -> Position {
        Position {
            symbol: "BTCUSDT".into(),
            position_side: "BOTH".into(),
            position_amt: amt,
            entry_price: entry,
            mark_price: mark,
            unrealized_pnl: pnl,
            leverage,
            liquidation_price: None,
            margin_type: "cross".into(),
            notional: (amt * mark).abs(),
            price_change_percent: 0.0,
            roe_percent: None,
        }
        .with_derived()
    }

    #[test]
    fn a_zero_row_is_not_an_open_position() {
        assert!(!position(0.0, 0.0, 0.0, 0.0, Some(10)).is_open());
        assert!(position(0.01, 60_000.0, 61_000.0, 10.0, Some(10)).is_open());
    }

    #[test]
    fn a_negative_amount_is_a_short() {
        assert!(position(0.01, 1.0, 1.0, 0.0, None).is_long());
        assert!(!position(-0.01, 1.0, 1.0, 0.0, None).is_long());
    }

    #[test]
    fn price_change_is_signed_by_the_direction_of_the_position() {
        // Long, price up 2% → +2%.
        let long = position(0.01, 60_000.0, 61_200.0, 12.0, Some(10));
        assert!((long.price_change_percent - 2.0).abs() < 1e-9);

        // Short, same price move → −2%: the market went the wrong way.
        let short = position(-0.01, 60_000.0, 61_200.0, -12.0, Some(10));
        assert!((short.price_change_percent + 2.0).abs() < 1e-9);

        // Short, price down 2% → +2%.
        let winning_short = position(-0.01, 60_000.0, 58_800.0, 12.0, Some(10));
        assert!((winning_short.price_change_percent - 2.0).abs() < 1e-9);
    }

    #[test]
    fn roe_is_pnl_over_committed_margin() {
        // 0.01 BTC at 61 200 = 612 notional; on 10x that is 61.2 of margin. A 12 USDT profit is
        // ~19.6% on the margin — close to the 2% price move times 10x, and deliberately derived
        // from the reported figures rather than from that multiplication.
        let long = position(0.01, 60_000.0, 61_200.0, 12.0, Some(10));
        let roe = long.roe_percent.unwrap();
        assert!((roe - 19.607_843).abs() < 1e-4, "{roe}");

        let leveraged_price_move = long.price_change_percent * 10.0;
        assert!(
            (roe - leveraged_price_move).abs() < 1.0,
            "ROE {roe} should sit near the leveraged price move {leveraged_price_move}"
        );
    }

    #[test]
    fn roe_is_unknown_rather_than_wrong_when_leverage_is_missing() {
        assert!(position(0.01, 60_000.0, 61_200.0, 12.0, None).roe_percent.is_none());
        assert!(position(0.01, 60_000.0, 61_200.0, 12.0, Some(0)).roe_percent.is_none());
    }

    #[test]
    fn an_entry_of_zero_does_not_divide_by_zero() {
        assert_eq!(position(0.01, 0.0, 100.0, 0.0, Some(5)).price_change_percent, 0.0);
    }
}
