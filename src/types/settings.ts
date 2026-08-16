import type { ChartType, Timeframe } from "./market";

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

export interface AppSettings {
  version: number;
  window: WindowSettings;
  display: DisplaySettings;
  chart: ChartSettings;
  watchlist: WatchlistItem[];
  alerts: Alert[];
  notifications: NotificationSettings;
  autostart: boolean;
}
