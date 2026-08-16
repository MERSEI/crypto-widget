import { create } from "zustand";
import type { ConnectionStatus, FxRate } from "../../types/market";

interface UiState {
  expanded: boolean;
  setExpanded: (expanded: boolean) => void;
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
