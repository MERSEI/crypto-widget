import { create } from "zustand";
import { commands } from "../ipc/commands";
import type { CoinInsight, InsightsState, MarketScan } from "../../types/insights";

/**
 * AI research state.
 *
 * The one rule this store exists to enforce on the JS side: **nothing here calls the model
 * without a click.** `load` and `showCoin` read the disk cache only; `research*` are the paid
 * paths and are reached from a button. `busy` is a single flag rather than one per card because
 * the backend also serialises calls — two spinners could never both be true.
 *
 * Reports are kept per asset (`BTC`, not `BTCUSDT`) to match the cache key in Rust, so asking
 * about the same coin from a different quote pair reuses the answer already paid for.
 */
interface InsightsStoreState {
  state: InsightsState | null;
  loaded: boolean;
  /** Which watchlist symbol the coin card is showing. */
  symbol: string | null;
  coin: CoinInsight | null;
  scan: MarketScan | null;
  busy: boolean;
  error: string | null;

  load: () => Promise<void>;
  saveSettings: (patch: Partial<InsightsState["settings"]>) => Promise<void>;
  saveKey: (apiKey: string) => Promise<void>;
  clearKey: () => Promise<void>;
  /** Switches the card to a symbol and shows its cached report, if any. Never spends. */
  showCoin: (symbol: string) => Promise<void>;
  researchCoin: (symbol: string, refresh: boolean) => Promise<void>;
  researchMarket: (refresh: boolean) => Promise<void>;
  openUrl: (url: string) => Promise<void>;
  dismissError: () => void;
}

export const useInsightsStore = create<InsightsStoreState>((set, get) => ({
  state: null,
  loaded: false,
  symbol: null,
  coin: null,
  scan: null,
  busy: false,
  error: null,

  load: async () => {
    const [state, scan] = await Promise.all([commands.getInsightsState(), commands.getCachedScan()]);
    set({ state, scan, loaded: true });
  },

  saveSettings: async (patch) => {
    const current = get().state;
    if (!current) return;
    const next = { ...current.settings, ...patch };
    try {
      set({
        state: await commands.setInsightsSettings(
          next.enabled,
          next.model,
          next.cacheTtlMin,
          next.maxSearches,
          next.language,
        ),
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveKey: async (apiKey) => {
    try {
      set({ state: await commands.setAnthropicKey(apiKey), error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearKey: async () => {
    try {
      set({ state: await commands.clearAnthropicKey(), error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  showCoin: async (symbol) => {
    set({ symbol, coin: null, error: null });
    const cached = await commands.getCachedInsight(symbol);
    // A slower answer for a symbol the user already moved away from must not overwrite the
    // card they are looking at now.
    if (get().symbol === symbol) set({ coin: cached });
  },

  researchCoin: async (symbol, refresh) => {
    if (get().busy) return;
    set({ busy: true, error: null, symbol });
    try {
      set({ coin: await commands.researchCoin(symbol, refresh) });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },

  researchMarket: async (refresh) => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try {
      set({ scan: await commands.researchMarket(refresh) });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },

  openUrl: async (url) => {
    try {
      await commands.openInsightUrl(url);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  dismissError: () => set({ error: null }),
}));
