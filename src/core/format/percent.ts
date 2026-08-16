export function formatPercent(percent: number): string {
  if (!Number.isFinite(percent)) return "--";
  const normalized = percent === 0 ? 0 : percent;
  const sign = normalized > 0 ? "+" : "";
  return `${sign}${normalized.toFixed(2)}%`;
}
