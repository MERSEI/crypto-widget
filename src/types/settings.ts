import type { ChartType, Timeframe } from "./market";
import type { WalletSettings } from "./wallet";

export type Edge = "right" | "left" | "top" | "bottom";

export interface WindowSettings {
  edge: Edge;
  offset: number;
  panelWidth: number;
  panelHeight: number;
  pinned: boolean;
}

export interface DisplaySettings {
  quote: string;
  fiat: string | null;
  theme: string;
}

export interface ChartSettings {
  defaultTimeframe: Timeframe;
  type: ChartType;
}

export interface WatchlistItem {
  symbol: string;
  order: number;
}

export type AlertKind = "price_above" | "price_below" | "spike";

export interface Alert {
  id: string;
  symbol: string;
  kind: AlertKind;
  value: number;
  windowMinutes: number | null;
  cooldownMin: number;
  once: boolean;
  enabled: boolean;
  lastFiredAt: string | null;
  /** Crossing state owned by the backend: true = waiting for the level to be crossed,
   *  false = level already taken, null = no price seen yet. */
  armed?: boolean | null;
}

export interface NotificationSettings {
  toast: boolean;
  sound: boolean;
}

export type VenueMode = "mainnet" | "testnet";

/** Non-secret half of the futures feature. API keys live in the OS credential store and never
 *  reach the renderer — mirrors `src-tauri/src/futures/mod.rs`. */
export interface FuturesSettings {
  enabled: boolean;
  mode: VenueMode;
  defaultLeverage: number;
  confirmOrders: boolean;
}

/** Mirrors `src-tauri/src/referral/mod.rs`. `ids` is keyed by partner id, so switching the
 *  dropdown doesn't discard the other partners' affiliate IDs. */
export interface ReferralSettings {
  partner: string | null;
  ids: Record<string, string>;
}

export interface AppSettings {
  version: number;
  window: WindowSettings;
  display: DisplaySettings;
  chart: ChartSettings;
  watchlist: WatchlistItem[];
  alerts: Alert[];
  notifications: NotificationSettings;
  autostart: boolean;
  futures: FuturesSettings;
  referral: ReferralSettings;
  wallet: WalletSettings;
}
