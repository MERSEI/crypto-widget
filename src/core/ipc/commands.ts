import { invoke } from "@tauri-apps/api/core";
import type { Candle, ConnectionStatus, FxRate, PairInfo, TickerSnapshot } from "../../types/market";
import type { Alert, AppSettings } from "../../types/settings";

export const commands = {
  getSettings: () => invoke<AppSettings>("get_settings"),
  getTickers: () => invoke<TickerSnapshot[]>("get_tickers"),
  getConnection: () => invoke<ConnectionStatus>("get_connection"),
  getExpanded: () => invoke<boolean>("get_expanded"),
  searchPairs: (query: string) => invoke<PairInfo[]>("search_pairs", { query }),
  getKlines: (symbol: string, interval: string) => invoke<Candle[]>("get_klines", { symbol, interval }),
  getFxRate: () => invoke<FxRate | null>("get_fx_rate"),

  addWatchlistSymbol: (symbol: string) => invoke<void>("add_watchlist_symbol", { symbol }),
  removeWatchlistSymbol: (symbol: string) => invoke<void>("remove_watchlist_symbol", { symbol }),
  reorderWatchlist: (symbols: string[]) => invoke<void>("reorder_watchlist", { symbols }),

  setDisplay: (quote: string, fiat: string | null) => invoke<void>("set_display", { quote, fiat }),
  setChartSettings: (defaultTimeframe: string, kind: string) =>
    invoke<void>("set_chart_settings", { defaultTimeframe, kind }),

  upsertAlert: (alert: Alert) => invoke<void>("upsert_alert", { alert }),
  deleteAlert: (id: string) => invoke<void>("delete_alert", { id }),

  setNotifications: (toast: boolean, sound: boolean) => invoke<void>("set_notifications", { toast, sound }),
  sendTestNotification: () => invoke<string>("send_test_notification"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),

  startDrag: () => invoke<void>("start_drag"),
  dragEnded: () => invoke<void>("drag_ended"),
  setPin: (pinned: boolean) => invoke<void>("set_pin", { pinned }),
  toggleExpand: () => invoke<boolean>("toggle_expand"),
  collapseIfUnpinned: () => invoke<void>("collapse_if_unpinned"),
};
