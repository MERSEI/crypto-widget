/** Mirrors `src-tauri/src/insights/mod.rs`. Change one, change the other.
 *
 *  The split between `analysis` and `fundamentals` is the point of these types and not an
 *  accident of layout: everything under `analysis` is a model's opinion, everything under
 *  `fundamentals` is a measured figure from CoinGecko, and the panel labels them differently
 *  because a reader has to be able to tell which is which at a glance. */

export type Verdict = "bullish" | "neutral" | "bearish";
export type Sentiment = "positive" | "neutral" | "negative";

/** Non-secret half of the feature. The Anthropic key lives in the OS credential store and
 *  never reaches the renderer — only its {@link CredentialStatus}. */
export interface InsightsSettings {
  enabled: boolean;
  model: string;
  cacheTtlMin: number;
  maxSearches: number;
  language: string;
}

export interface CredentialStatus {
  present: boolean;
  maskedKey: string | null;
}

export interface InsightsState {
  settings: InsightsSettings;
  key: CredentialStatus;
  /** The models Rust will accept, in the order the dropdown should offer them. */
  models: string[];
}

export interface NewsItem {
  title: string;
  url: string;
  source: string;
  published: string | null;
  sentiment: Sentiment;
  impact: string;
}

export interface Source {
  title: string;
  url: string;
}

export interface Fundamentals {
  id: string;
  name: string;
  symbol: string;
  marketCapUsd: number | null;
  marketCapRank: number | null;
  volume24hUsd: number | null;
  circulatingSupply: number | null;
  maxSupply: number | null;
  athUsd: number | null;
  athChangePct: number | null;
  categories: string[];
  homepage: string | null;
  github: string | null;
  sentimentUpPct: number | null;
}

/** What one call cost, in the units the API reports. Shown in the card footer so the spend is
 *  visible where it happens. */
export interface Usage {
  inputTokens: number;
  outputTokens: number;
  webSearches: number;
}

export interface Analysis {
  verdict: Verdict;
  /** 0–100 "how interesting right now" — not a price target and not a probability. */
  score: number;
  summary: string;
  catalysts: string[];
  risks: string[];
  news: NewsItem[];
}

export interface CoinInsight {
  symbol: string;
  asset: string;
  analysis: Analysis;
  fundamentals: Fundamentals | null;
  sources: Source[];
  /** Unix ms of the call that produced this, not of this delivery. */
  generatedAt: number;
  model: string;
  usage: Usage;
  cached: boolean;
}

export interface ProjectIdea {
  name: string;
  symbol: string;
  category: string;
  thesis: string;
  catalyst: string;
  risk: string;
  horizon: string;
  conviction: number;
  url: string | null;
  /** Resolved in Rust against Binance's pair list, never taken from the model — `null` means
   *  the widget cannot price it, so there is no "add to watchlist" button. */
  binanceSymbol: string | null;
}

export interface TrendingCoin {
  id: string;
  name: string;
  symbol: string;
  marketCapRank: number | null;
  binanceSymbol: string | null;
}

export interface MarketScan {
  narrative: string;
  ideas: ProjectIdea[];
  trending: TrendingCoin[];
  sources: Source[];
  generatedAt: number;
  model: string;
  usage: Usage;
  cached: boolean;
}
