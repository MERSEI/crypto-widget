import { create } from "zustand";
import type { ConnectionStatus, FxRate } from "../../types/market";

/** Which panel tab is showing. Deliberately not persisted: the widget's job is to answer
 *  "where's the market right now" on sight, so it always reopens on the watchlist. */
export type PanelTab = "watch" | "ai" | "futures" | "referral";

interface UiState {
  expanded: boolean;
  setExpanded: (expanded: boolean) => void;
  activeTab: PanelTab;
  setActiveTab: (tab: PanelTab) => void;
  openSymbol: string | null;
  toggleOpenSymbol: (symbol: string) => void;
  searchOpen: boolean;
  setSearchOpen: (open: boolean) => void;
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  connection: ConnectionStatus;
  setConnection: (status: ConnectionStatus) => void;

  fxRate: FxRate | null;
  setFxRate: (rate: FxRate | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  expanded: false,
  setExpanded: (expanded) => set({ expanded }),

  activeTab: "watch",
  setActiveTab: (activeTab) => set({ activeTab }),

  openSymbol: null,
  toggleOpenSymbol: (symbol) =>
    set((state) => ({ openSymbol: state.openSymbol === symbol ? null : symbol })),

  searchOpen: false,
  setSearchOpen: (searchOpen) => set({ searchOpen }),

  settingsOpen: false,
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),

  connection: { state: "connecting", attempt: 0, latencyMs: null },
  setConnection: (connection) => set({ connection }),

  fxRate: null,
  setFxRate: (fxRate) => set({ fxRate }),
}));
