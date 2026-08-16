import { create } from "zustand";
import { commands } from "../ipc/commands";
import type { Alert } from "../../types/settings";

interface AlertsState {
  items: Alert[];
  setItems: (items: Alert[]) => void;
  upsert: (alert: Alert) => Promise<void>;
  remove: (id: string) => Promise<void>;
  forSymbol: (symbol: string) => Alert[];
}

export const useAlertsStore = create<AlertsState>((set, get) => ({
  items: [],
  setItems: (items) => set({ items }),

  upsert: async (alert) => {
    await commands.upsertAlert(alert);
    set((state) => {
      const exists = state.items.some((a) => a.id === alert.id);
      return {
        items: exists
          ? state.items.map((a) => (a.id === alert.id ? alert : a))
          : [...state.items, alert],
      };
    });
  },

  remove: async (id) => {
    await commands.deleteAlert(id);
    set((state) => ({ items: state.items.filter((a) => a.id !== id) }));
  },

  forSymbol: (symbol) => get().items.filter((a) => a.symbol === symbol),
}));
