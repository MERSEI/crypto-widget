import { describe, expect, it } from "vitest";
import { formatPrice } from "../src/core/format/price";
import { formatPercent } from "../src/core/format/percent";
import { formatVolume } from "../src/core/format/volume";

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
