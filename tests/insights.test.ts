import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CoinInsight } from "../src/types/insights";

// The IPC layer is the thing under test here — not what Rust does with the call, but *whether*
// a call is made. Every assertion below is about money: a research call that happens without a
// click is a charge the user did not ask for.
const researchCoin = vi.fn();
const getCachedInsight = vi.fn();
const researchMarket = vi.fn();
const openInsightUrl = vi.fn();

vi.mock("../src/core/ipc/commands", () => ({
  commands: {
    researchCoin: (...args: unknown[]) => researchCoin(...args),
    getCachedInsight: (...args: unknown[]) => getCachedInsight(...args),
    researchMarket: (...args: unknown[]) => researchMarket(...args),
    openInsightUrl: (...args: unknown[]) => openInsightUrl(...args),
  },
}));

const { useInsightsStore } = await import("../src/core/store/insights");

function insight(symbol: string): CoinInsight {
  return {
    symbol,
    asset: symbol.replace("USDT", ""),
    analysis: { verdict: "neutral", score: 50, summary: "", catalysts: [], risks: [], news: [] },
    fundamentals: null,
    sources: [],
    generatedAt: 1,
    model: "claude-opus-5",
    usage: { inputTokens: 0, outputTokens: 0, webSearches: 0 },
    cached: false,
  };
}

describe("useInsightsStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useInsightsStore.setState({ symbol: null, coin: null, scan: null, busy: false, error: null });
  });

  it("shows a cached report without calling the model", async () => {
    getCachedInsight.mockResolvedValue(insight("BTCUSDT"));

    await useInsightsStore.getState().showCoin("BTCUSDT");

    expect(getCachedInsight).toHaveBeenCalledWith("BTCUSDT");
    expect(researchCoin).not.toHaveBeenCalled();
    expect(useInsightsStore.getState().coin?.symbol).toBe("BTCUSDT");
  });

  it("does not let a late cache read overwrite the card the user moved to", async () => {
    // Switching symbols twice quickly used to leave the slower answer on screen — under a
    // different symbol's heading, which is the worst possible way to read a research report.
    let resolveFirst: (value: CoinInsight) => void = () => {};
    getCachedInsight
      .mockImplementationOnce(() => new Promise<CoinInsight>((resolve) => (resolveFirst = resolve)))
      .mockResolvedValueOnce(insight("ETHUSDT"));

    const first = useInsightsStore.getState().showCoin("BTCUSDT");
    await useInsightsStore.getState().showCoin("ETHUSDT");
    resolveFirst(insight("BTCUSDT"));
    await first;

    expect(useInsightsStore.getState().symbol).toBe("ETHUSDT");
    expect(useInsightsStore.getState().coin?.symbol).toBe("ETHUSDT");
  });

  it("refuses a second call while one is in flight", async () => {
    researchCoin.mockImplementation(() => new Promise(() => {}));

    void useInsightsStore.getState().researchCoin("BTCUSDT", false);
    await useInsightsStore.getState().researchCoin("BTCUSDT", false);
    await useInsightsStore.getState().researchMarket(false);

    expect(researchCoin).toHaveBeenCalledTimes(1);
    expect(researchMarket).not.toHaveBeenCalled();
  });

  it("surfaces a failed call and stops being busy", async () => {
    researchCoin.mockRejectedValue("the API key was rejected");

    await useInsightsStore.getState().researchCoin("BTCUSDT", true);

    expect(useInsightsStore.getState().error).toContain("rejected");
    expect(useInsightsStore.getState().busy).toBe(false);
  });

  it("reports a refused link instead of failing silently", async () => {
    // Rust refuses any URL that is not in a stored report; the panel has to say so rather than
    // leave a click looking like it worked.
    openInsightUrl.mockRejectedValue("refusing to open a link that is not in a stored report");

    await useInsightsStore.getState().openUrl("https://example.com");

    expect(useInsightsStore.getState().error).toContain("refusing to open");
  });
});
