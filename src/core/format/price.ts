/**
 * Formats a price keeping roughly 4-5 significant digits regardless of magnitude, so
 * 109240.5, 3.9421 and 0.00002841 all read naturally in a fixed-width tabular-nums column.
 */
export function formatPrice(price: number): string {
  if (!Number.isFinite(price)) return "--";
  const abs = Math.abs(price);
  if (abs === 0) return "0.00";

  let decimals: number;
  if (abs >= 1000) {
    decimals = 2;
  } else if (abs >= 1) {
    decimals = 4;
  } else {
    const leadingZeros = Math.max(0, -Math.floor(Math.log10(abs)) - 1);
    decimals = leadingZeros + 4;
  }

  const normalized = price === 0 ? 0 : price; // collapse -0 to 0
  return normalized.toFixed(decimals);
}
