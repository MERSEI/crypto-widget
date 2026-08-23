import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetBalance, WalletState } from "../src/types/wallet";

const getWalletBalances = vi.fn();
const getWalletHistory = vi.fn();
const getWalletState = vi.fn();

vi.mock("../src/core/ipc/commands", () => ({
  commands: {
    getWalletBalances: (...args: unknown[]) => getWalletBalances(...args),
    getWalletHistory: (...args: unknown[]) => getWalletHistory(...args),
    getWalletState: (...args: unknown[]) => getWalletState(...args),
  },
}));

const { useWalletStore, walletErrorMessage } = await import("../src/core/store/wallet");

function walletState(initialized = true): WalletState {
  return {
    status: {
      initialized,
      address: initialized ? "0x9858EfFD232B4033E47d90003D41EC34EcaEda94" : null,
      accountIndex: 0,
    },
    settings: {
      rpcUrl: "https://ethereum-rpc.publicnode.com",
      chainId: 1,
      accountIndex: 0,
      tokens: [],
      widgetEnabled: true,
    },
    etherscan: { present: false, maskedKey: null },
  };
}

function balance(overrides: Partial<AssetBalance> = {}): AssetBalance {
  return {
    contract: null,
    symbol: "ETH",
    decimals: 18,
    raw: "1000000000000000000",
    amount: "1.0",
    ...overrides,
  };
}

describe("useWalletStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWalletStore.setState({
      state: null,
      balances: [],
      history: [],
      balancesLoading: false,
      historyLoading: false,
      balancesError: null,
      historyError: null,
    });
  });

  it("does not call the backend before a wallet exists", async () => {
    useWalletStore.setState({ state: walletState(false) });
    await useWalletStore.getState().refreshBalances();
    await useWalletStore.getState().refreshHistory();
    expect(getWalletBalances).not.toHaveBeenCalled();
    expect(getWalletHistory).not.toHaveBeenCalled();
  });

  it("keeps the last good balances when a refresh fails", async () => {
    useWalletStore.setState({ state: walletState() });
    getWalletBalances.mockResolvedValueOnce([balance()]);
    await useWalletStore.getState().refreshBalances();

    // A dropped RPC connection that blanked the list would look exactly like an emptied wallet.
    getWalletBalances.mockRejectedValueOnce("could not read the balance");
    await useWalletStore.getState().refreshBalances();

    expect(useWalletStore.getState().balances).toHaveLength(1);
    expect(useWalletStore.getState().balancesError).toBe("could not read the balance");
    expect(useWalletStore.getState().balancesLoading).toBe(false);
  });

  it("keeps balances and history failures apart", async () => {
    useWalletStore.setState({ state: walletState() });
    getWalletBalances.mockResolvedValueOnce([balance()]);
    getWalletHistory.mockRejectedValueOnce("add an Etherscan API key");

    await useWalletStore.getState().refreshBalances();
    await useWalletStore.getState().refreshHistory();

    // A wallet without a history key is perfectly usable; the missing key may not take the
    // balances down with it.
    expect(useWalletStore.getState().balancesError).toBeNull();
    expect(useWalletStore.getState().balances).toHaveLength(1);
    expect(useWalletStore.getState().historyError).toBe("add an Etherscan API key");
  });

  it("hydrates balances only for a wallet that exists", async () => {
    getWalletState.mockResolvedValueOnce(walletState(false));
    await useWalletStore.getState().hydrate();
    expect(getWalletBalances).not.toHaveBeenCalled();

    getWalletState.mockResolvedValueOnce(walletState());
    getWalletBalances.mockResolvedValueOnce([balance()]);
    await useWalletStore.getState().hydrate();
    expect(getWalletBalances).toHaveBeenCalledTimes(1);
  });
});

describe("walletErrorMessage", () => {
  it("passes a rejected command string through as written", () => {
    // Tauri rejects with whatever the command returned — a plain string. Wrapping it in
    // `String(new Error(...))` would prefix every backend message with "Error: ".
    expect(walletErrorMessage("the seed phrase checksum does not match")).toBe(
      "the seed phrase checksum does not match",
    );
    expect(walletErrorMessage(new Error("boom"))).toBe("boom");
  });
});
