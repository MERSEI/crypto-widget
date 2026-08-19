import { invoke } from "@tauri-apps/api/core";
import type { FuturesKeyStatus, FuturesState, VenueMode } from "../../types/futures";
import type { Candle, ConnectionStatus, FxRate, PairInfo, TickerSnapshot } from "../../types/market";
import type { Partner, ReferralProfile } from "../../types/referral";
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

  // Referral. Every mutation answers with the rebuilt profile, so the link (or the reason
  // there isn't one) never has to be recomputed on the JS side.
  getReferralPartners: () => invoke<Partner[]>("get_referral_partners"),
  getReferralProfile: () => invoke<ReferralProfile>("get_referral_profile"),
  setReferralPartner: (partner: string | null) => invoke<ReferralProfile>("set_referral_partner", { partner }),
  setReferralId: (partner: string, id: string) => invoke<ReferralProfile>("set_referral_id", { partner, id }),
  setReferralTemplate: (partner: string, template: string) =>
    invoke<ReferralProfile>("set_referral_template", { partner, template }),
  openReferralUrl: (url: string) => invoke<void>("open_referral_url", { url }),

  // Futures. The API secret crosses this boundary exactly once — in `setFuturesKeys` — and is
  // never sent back; everything else returns masked status or public account figures.
  getFuturesState: () => invoke<FuturesState>("get_futures_state"),
  getFuturesKeys: () => invoke<FuturesKeyStatus>("get_futures_keys"),
  setFuturesKeys: (venue: VenueMode, apiKey: string, apiSecret: string) =>
    invoke<FuturesKeyStatus>("set_futures_keys", { venue, apiKey, apiSecret }),
  clearFuturesKeys: (venue: VenueMode) => invoke<FuturesKeyStatus>("clear_futures_keys", { venue }),
  setFuturesEnabled: (enabled: boolean) => invoke<FuturesState>("set_futures_enabled", { enabled }),
  setFuturesVenue: (venue: VenueMode) => invoke<FuturesState>("set_futures_venue", { venue }),
  setFuturesPreferences: (defaultLeverage: number, confirmOrders: boolean) =>
    invoke<FuturesState>("set_futures_preferences", { defaultLeverage, confirmOrders }),
  refreshFutures: () => invoke<FuturesState>("refresh_futures"),
  testFuturesConnection: () => invoke<string>("test_futures_connection"),
  openFuturesWindow: () => invoke<void>("open_futures_window"),

  startDrag: () => invoke<void>("start_drag"),
  dragEnded: () => invoke<void>("drag_ended"),
  setPin: (pinned: boolean) => invoke<void>("set_pin", { pinned }),
  toggleExpand: () => invoke<boolean>("toggle_expand"),
  collapseIfUnpinned: () => invoke<void>("collapse_if_unpinned"),
};
