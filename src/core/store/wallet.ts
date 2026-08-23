import { create } from "zustand";
import { commands } from "../ipc/commands";
import type { AssetBalance, Transfer, WalletState } from "../../types/wallet";

/**
 * Wallet mirror.
 *
 * Unlike the futures store there is no backend event to subscribe to: balances cost an RPC
 * round trip and history costs an Etherscan request against a five-per-second budget, so both
 * are pulled when the user asks and after anything that changes them. `loading` and `error` are
 * per-slice for that reason — a rate-limited history must not blank out balances that loaded
 * fine.
 *
 * Nothing here computes with an amount. Every figure arrives formatted from Rust; the moment
 * this file did the arithmetic, it would do it in floating point.
 */
interface WalletStoreState {
  state: WalletState | null;
  balances: AssetBalance[];
  history: Transfer[];
  balancesLoading: boolean;
  historyLoading: boolean;
  balancesError: string | null;
  historyError: string | null;

  hydrate: () => Promise<void>;
  apply: (state: WalletState) => void;
  refreshBalances: () => Promise<void>;
  refreshHistory: (limit?: number) => Promise<void>;
}

/** Tauri rejects with whatever the command returned — a plain string here, since every wallet
 *  command answers `Result<_, String>`. `String(e)` on an Error would read "Error: …". */
function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export const useWalletStore = create<WalletStoreState>((set, get) => ({
  state: null,
  balances: [],
  history: [],
  balancesLoading: false,
  historyLoading: false,
  balancesError: null,
  historyError: null,

  hydrate: async () => {
    const state = await commands.getWalletState();
    set({ state });
    if (state.status.initialized) {
      void get().refreshBalances();
    }
  },

  apply: (state) => set({ state }),

  refreshBalances: async () => {
    if (!get().state?.status.initialized) return;
    set({ balancesLoading: true, balancesError: null });
    try {
      set({ balances: await commands.getWalletBalances() });
    } catch (e) {
      // The previous balances stay on screen next to the error: a wallet that blanks out on a
      // dropped RPC connection looks exactly like a wallet that was emptied.
      set({ balancesError: message(e) });
    } finally {
      set({ balancesLoading: false });
    }
  },

  refreshHistory: async (limit) => {
    if (!get().state?.status.initialized) return;
    set({ historyLoading: true, historyError: null });
    try {
      set({ history: await commands.getWalletHistory(limit) });
    } catch (e) {
      set({ historyError: message(e) });
    } finally {
      set({ historyLoading: false });
    }
  },
}));

export { message as walletErrorMessage };
