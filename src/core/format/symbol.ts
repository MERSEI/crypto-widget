/** Quote assets the widget knows how to split off a Binance-style symbol, longest first — the
 *  same list as `KNOWN_QUOTES` in `src-tauri/src/market/mod.rs`. Order matters: `GRAMUSDT` has
 *  to split as GRAM/USDT and not GRAMU/SDT. */
const KNOWN_QUOTES = ["FDUSD", "USDT", "USDC", "USD1", "TRY", "BTC"];

/**
 * `BTCUSDT` → `BTC`: the asset a pair prices, for labels that are about the coin rather than
 * the market. A symbol with no known quote suffix is already an asset name and comes back
 * unchanged.
 */
export function baseAsset(symbol: string): string {
  const upper = symbol.trim().toUpperCase();
  const quote = KNOWN_QUOTES.filter((q) => upper.length > q.length && upper.endsWith(q)).sort(
    (a, b) => b.length - a.length,
  )[0];
  return quote ? upper.slice(0, upper.length - quote.length) : upper;
}
