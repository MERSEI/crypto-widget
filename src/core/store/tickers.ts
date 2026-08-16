import { create } from "zustand";
import { STALE_AFTER_MS, type TickerSnapshot } from "../../types/market";

/** How far `eventTime` may drift before a snapshot is stored even though the numbers are
 *  identical — otherwise a quiet pair priced by a backup venue would age into the STALE badge
 *  while a perfectly fresh quote sat in the update being skipped. Half the staleness window,
 *  so the refresh always lands before the badge would appear. */
const FRESHNESS_REFRESH_MS = STALE_AFTER_MS / 2;

interface TickersState {
  bySymbol: Record<string, TickerSnapshot>;
  applySnapshot: (list: TickerSnapshot[]) => void;
}

export const useTickersStore = create<TickersState>((set) => ({
  bySymbol: {},
  // Only touches symbols whose price/percent actually moved, so a BTC tick doesn't
  // recreate the object identity ETH's selector is watching.
  applySnapshot: (list) =>
    set((state) => {
      let changed = false;
      const bySymbol = { ...state.bySymbol };
      for (const ticker of list) {
        const prev = bySymbol[ticker.symbol];
        if (
          !prev ||
          prev.price !== ticker.price ||
          prev.percent24h !== ticker.percent24h ||
          prev.source !== ticker.source ||
          ticker.eventTime - prev.eventTime >= FRESHNESS_REFRESH_MS
        ) {
          bySymbol[ticker.symbol] = ticker;
          changed = true;
        }
      }
      return changed ? { bySymbol } : state;
    }),
}));
