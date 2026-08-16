export interface TickerSnapshot {
  symbol: string;
  price: number;
  percent24h: number;
  quoteVolume: number;
  eventTime: number;
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
