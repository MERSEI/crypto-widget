import { describe, expect, it } from "vitest";
import { formatPrice } from "../src/core/format/price";
import { formatPercent } from "../src/core/format/percent";
import { formatVolume } from "../src/core/format/volume";
import { formatAge } from "../src/core/format/age";
import { baseAsset } from "../src/core/format/symbol";

describe("formatPrice", () => {
  it("shows 2 decimals for large prices", () => {
    expect(formatPrice(109240.5)).toBe("109240.50");
  });

  it("shows 4 decimals for mid-range prices", () => {
    expect(formatPrice(3.9421)).toBe("3.9421");
  });

  it("preserves significant digits for tiny prices", () => {
    expect(formatPrice(0.00002841)).toBe("0.00002841");
  });

  it("collapses negative zero", () => {
    expect(formatPrice(-0)).toBe("0.00");
  });

  it("handles non-finite input", () => {
    expect(formatPrice(NaN)).toBe("--");
  });
});

describe("formatPercent", () => {
  it("prefixes positive values with +", () => {
    expect(formatPercent(2.345)).toBe("+2.35%");
  });

  it("keeps the sign for negative values", () => {
    expect(formatPercent(-1.05)).toBe("-1.05%");
  });

  it("collapses negative zero", () => {
    expect(formatPercent(-0)).toBe("0.00%");
  });
});

describe("formatVolume", () => {
  it("abbreviates billions", () => {
    expect(formatVolume(1_200_000_000)).toBe("1.2B");
  });

  it("abbreviates millions", () => {
    expect(formatVolume(340_000_000)).toBe("340.0M");
  });

  it("abbreviates thousands", () => {
    expect(formatVolume(12_300)).toBe("12.3K");
  });

  it("leaves small numbers as-is", () => {
    expect(formatVolume(42)).toBe("42");
  });
});

describe("formatAge", () => {
  const now = Date.parse("2026-08-31T12:00:00Z");

  it("calls anything under a minute fresh", () => {
    expect(formatAge(now - 40_000, now)).toBe("just now");
  });

  it("rounds down rather than up", () => {
    // 119 minutes is 1h, not 2h: an AI report's age is the reason to refresh it, so an
    // optimistic reading of it is the wrong kind of wrong.
    expect(formatAge(now - 119 * 60_000, now)).toBe("1h ago");
  });

  it("switches units at the boundaries", () => {
    expect(formatAge(now - 60_000, now)).toBe("1m ago");
    expect(formatAge(now - 60 * 60_000, now)).toBe("1h ago");
    expect(formatAge(now - 24 * 60 * 60_000, now)).toBe("1d ago");
  });

  it("does not read a clock that ran backwards as an old report", () => {
    expect(formatAge(now + 5_000, now)).toBe("just now");
  });

  it("says so when there is no timestamp", () => {
    expect(formatAge(0, now)).toBe("unknown age");
  });
});

describe("baseAsset", () => {
  it("splits the longest matching quote, not the first", () => {
    // GRAMUSDT must not become GRAMU/SDT — the same trap `KNOWN_QUOTES` guards in Rust.
    expect(baseAsset("GRAMUSDT")).toBe("GRAM");
    expect(baseAsset("ETHFDUSD")).toBe("ETH");
  });

  it("leaves a bare asset name alone", () => {
    expect(baseAsset("btc")).toBe("BTC");
  });
});
