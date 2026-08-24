/** Mirrors the Rust types in `src-tauri/src/futures/` — change them in pairs. */

export type VenueMode = "mainnet" | "testnet";

export type FuturesStatus = "off" | "nokey" | "connecting" | "live" | "error";

export interface Position {
  symbol: string;
  /** `LONG`, `SHORT` or `BOTH` (one-way mode). */
  positionSide: string;
  /** Signed: negative is short. */
  positionAmt: number;
  entryPrice: number;
  markPrice: number;
  unrealizedPnl: number;
  leverage: number | null;
  /** Null when the venue reports no liquidation price for the position. */
  liquidationPrice: number | null;
  marginType: string;
  notional: number;
  /** Price move from entry, signed by direction, leverage-independent. Computed in Rust. */
  priceChangePercent: number;
  /** Return on committed margin. Null when leverage is unknown. Computed in Rust. */
  roePercent: number | null;
}

export interface AccountSummary {
  totalWalletBalance: number;
  totalUnrealizedPnl: number;
  totalMarginBalance: number;
  availableBalance: number;
}

export interface FuturesSnapshot {
  account: AccountSummary;
  positions: Position[];
  updatedAt: number;
}

export interface FuturesState {
  status: FuturesStatus;
  mode: VenueMode;
  message: string | null;
  /** Kept through an error so the panel never blanks. */
  snapshot: FuturesSnapshot | null;
  /** Whether orders may be placed — testnet only, enforced in Rust. */
  tradingAllowed: boolean;
}

/** Never carries a secret: only whether one is stored, and a masked hint. */
export interface CredentialStatus {
  present: boolean;
  maskedKey: string | null;
}

export interface FuturesKeyStatus {
  mainnet: CredentialStatus;
  testnet: CredentialStatus;
}

export type OrderSide = "buy" | "sell";
export type OrderKind = "market" | "limit";

export interface OrderReceipt {
  orderId: number;
  symbol: string;
  status: string;
  avgPrice: number;
  executedQty: number;
}

export interface OrderRecord {
  orderId: number;
  symbol: string;
  side: string;
  orderType: string;
  status: string;
  price: number;
  avgPrice: number;
  origQty: number;
  executedQty: number;
  reduceOnly: boolean;
  /** Unix ms. */
  time: number;
}
