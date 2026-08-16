/** A price older than this is shown as stale rather than as a live quote. Mirrors
 *  `STALE_AFTER_MS` in `src-tauri/src/market/mod.rs`, which also gates the alert engine. */
export const STALE_AFTER_MS = 60_000;

export interface TickerSnapshot {
  symbol: string;
  price: number;
  percent24h: number;
  quoteVolume: number;
  eventTime: number;
  /** Venue the price came from: "binance" for the primary feed, otherwise a backup venue. */
  source: string;
}

export interface Candle {
  time: number; // unix seconds
  open: number;
  high: number;
  low: number;
  close: number;
}

export interface PairInfo {
  symbol: string;
  baseAsset: string;
  quoteVolume: number;
}

export type ConnectionState = "connecting" | "live" | "stale" | "reconnecting" | "polling" | "offline";

export interface ConnectionStatus {
  state: ConnectionState;
  attempt: number;
  latencyMs: number | null;
}

export interface FxRate {
  pair: string;
  rate: number;
  asOf: string;
}

export type Timeframe = "1h" | "4h" | "1d" | "1w";
export type ChartType = "area" | "candlestick";
