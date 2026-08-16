import { create } from "zustand";
import { commands } from "../ipc/commands";
import type { AppSettings, ChartSettings, DisplaySettings, WindowSettings } from "../../types/settings";

interface SettingsState {
  settings: AppSettings | null;
  setSettings: (settings: AppSettings) => void;
  setWindow: (window: WindowSettings) => void;
  setDisplay: (quote: string, fiat: string | null) => Promise<void>;
  setChart: (chart: ChartSettings) => Promise<void>;
  setNotifications: (toast: boolean, sound: boolean) => Promise<void>;
  setAutostart: (enabled: boolean) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: null,
  setSettings: (settings) => set({ settings }),
  setWindow: (window) => set((state) => (state.settings ? { settings: { ...state.settings, window } } : state)),

  setDisplay: async (quote, fiat) => {
    await commands.setDisplay(quote, fiat);
    const current = get().settings;
    if (current) set({ settings: { ...current, display: { ...current.display, quote, fiat } as DisplaySettings } });
  },

  setChart: async (chart) => {
    await commands.setChartSettings(chart.defaultTimeframe, chart.type);
    const current = get().settings;
    if (current) set({ settings: { ...current, chart } });
  },

  setNotifications: async (toast, sound) => {
    await commands.setNotifications(toast, sound);
    const current = get().settings;
    if (current) set({ settings: { ...current, notifications: { toast, sound } } });
  },

  setAutostart: async (enabled) => {
    await commands.setAutostart(enabled);
    const current = get().settings;
    if (current) set({ settings: { ...current, autostart: enabled } });
  },
}));
