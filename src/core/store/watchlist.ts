import { create } from "zustand";
import { commands } from "../ipc/commands";
import type { WatchlistItem } from "../../types/settings";

interface WatchlistState {
  items: WatchlistItem[];
  loaded: boolean;
  setItems: (items: WatchlistItem[]) => void;
  add: (symbol: string) => Promise<void>;
  remove: (symbol: string) => Promise<void>;
  reorder: (symbols: string[]) => Promise<void>;
}

export const useWatchlistStore = create<WatchlistState>((set, get) => ({
  items: [],
  loaded: false,
  setItems: (items) => set({ items: [...items].sort((a, b) => a.order - b.order), loaded: true }),

  add: async (symbol) => {
    await commands.addWatchlistSymbol(symbol);
    const upper = symbol.toUpperCase();
    if (!get().items.some((item) => item.symbol === upper)) {
      set((state) => ({ items: [...state.items, { symbol: upper, order: state.items.length }] }));
    }
  },

  remove: async (symbol) => {
    await commands.removeWatchlistSymbol(symbol);
    set((state) => ({
      items: state.items
        .filter((item) => item.symbol !== symbol)
        .map((item, order) => ({ ...item, order })),
    }));
  },

  reorder: async (symbols) => {
    await commands.reorderWatchlist(symbols);
    set((state) => {
      const bySymbol = new Map(state.items.map((item) => [item.symbol, item]));
      const items = symbols
        .map((symbol, order) => {
          const existing = bySymbol.get(symbol);
          return existing ? { ...existing, order } : null;
        })
        .filter((item): item is WatchlistItem => item !== null);
      return { items };
    });
  },
}));
