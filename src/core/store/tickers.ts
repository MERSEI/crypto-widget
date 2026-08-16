import { create } from "zustand";
import type { TickerSnapshot } from "../../types/market";

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
        if (!prev || prev.price !== ticker.price || prev.percent24h !== ticker.percent24h) {
          bySymbol[ticker.symbol] = ticker;
          changed = true;
        }
      }
      return changed ? { bySymbol } : state;
    }),
}));
