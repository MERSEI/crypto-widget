import { create } from "zustand";
import { commands } from "../ipc/commands";
import type { FuturesKeyStatus, FuturesState, VenueMode } from "../../types/futures";

/**
 * Futures account mirror.
 *
 * Rust polls the exchange and emits `futures`; this store holds the last state it was handed.
 * Nothing here derives a number — PnL, ROE and the price move all arrive computed, so the panel
 * and the backend can never disagree about what a position is worth.
 */
interface FuturesStoreState {
  state: FuturesState | null;
  keys: FuturesKeyStatus | null;
  /** Result of the last "Test connection", cleared when the venue or key changes. */
  testResult: string | null;
  hydrate: () => Promise<void>;
  apply: (state: FuturesState) => void;
  setEnabled: (enabled: boolean) => Promise<void>;
  setVenue: (venue: VenueMode) => Promise<void>;
  saveKeys: (venue: VenueMode, apiKey: string, apiSecret: string) => Promise<void>;
  clearKeys: (venue: VenueMode) => Promise<void>;
  refresh: () => Promise<void>;
  testConnection: () => Promise<void>;
}

export const useFuturesStore = create<FuturesStoreState>((set) => ({
  state: null,
  keys: null,
  testResult: null,

  hydrate: async () => {
    const [state, keys] = await Promise.all([commands.getFuturesState(), commands.getFuturesKeys()]);
    set({ state, keys });
  },

  apply: (state) => set({ state }),

  setEnabled: async (enabled) => {
    set({ state: await commands.setFuturesEnabled(enabled) });
  },

  setVenue: async (venue) => {
    set({ state: await commands.setFuturesVenue(venue), testResult: null });
  },

  saveKeys: async (venue, apiKey, apiSecret) => {
    const keys = await commands.setFuturesKeys(venue, apiKey, apiSecret);
    set({ keys, testResult: null, state: await commands.getFuturesState() });
  },

  clearKeys: async (venue) => {
    const keys = await commands.clearFuturesKeys(venue);
    set({ keys, testResult: null, state: await commands.getFuturesState() });
  },

  refresh: async () => {
    // Never rejects: a failed poll comes back inside the state, next to the last good snapshot.
    set({ state: await commands.refreshFutures() });
  },

  testConnection: async () => {
    try {
      set({ testResult: await commands.testFuturesConnection() });
    } catch (e) {
      set({ testResult: String(e) });
    }
  },
}));
