import { beforeEach, describe, expect, it } from "vitest";
import { useTickersStore } from "../src/core/store/tickers";
import type { TickerSnapshot } from "../src/types/market";

function ticker(overrides: Partial<TickerSnapshot> = {}): TickerSnapshot {
  return {
    symbol: "BTCUSDT",
    price: 100,
    percent24h: 1,
    quoteVolume: 1000,
    eventTime: 1,
    ...overrides,
  };
}

describe("useTickersStore", () => {
  beforeEach(() => {
    useTickersStore.setState({ bySymbol: {} });
  });

  it("stores an incoming snapshot", () => {
    useTickersStore.getState().applySnapshot([ticker()]);
    expect(useTickersStore.getState().bySymbol.BTCUSDT.price).toBe(100);
  });

  it("does not recreate bySymbol when nothing changed", () => {
    useTickersStore.getState().applySnapshot([ticker()]);
    const before = useTickersStore.getState().bySymbol;
    useTickersStore.getState().applySnapshot([ticker()]); // identical price/percent
    const after = useTickersStore.getState().bySymbol;
    expect(after).toBe(before);
  });

  it("recreates bySymbol when a price actually moves", () => {
    useTickersStore.getState().applySnapshot([ticker()]);
    const before = useTickersStore.getState().bySymbol;
    useTickersStore.getState().applySnapshot([ticker({ price: 101 })]);
    const after = useTickersStore.getState().bySymbol;
    expect(after).not.toBe(before);
    expect(after.BTCUSDT.price).toBe(101);
  });

  it("leaves an unrelated symbol's object identity untouched", () => {
    useTickersStore.getState().applySnapshot([ticker({ symbol: "ETHUSDT", price: 50 })]);
    const ethBefore = useTickersStore.getState().bySymbol.ETHUSDT;
    useTickersStore.getState().applySnapshot([
      ticker({ symbol: "ETHUSDT", price: 50 }),
      ticker({ symbol: "BTCUSDT", price: 200 }),
    ]);
    expect(useTickersStore.getState().bySymbol.ETHUSDT).toBe(ethBefore);
  });
});
