export function formatVolume(volume: number): string {
  if (!Number.isFinite(volume)) return "--";
  const abs = Math.abs(volume);
  if (abs >= 1_000_000_000) return `${(volume / 1_000_000_000).toFixed(1)}B`;
  if (abs >= 1_000_000) return `${(volume / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${(volume / 1_000).toFixed(1)}K`;
  return volume.toFixed(0);
}
