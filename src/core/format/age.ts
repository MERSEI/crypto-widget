/**
 * How long ago something was produced, in the coarsest unit that is still honest.
 *
 * Used on AI reports, where the age is the single most important thing about the answer: a
 * verdict written four hours ago is a different claim from the same verdict written a minute
 * ago, and the panel serves cached reports by design. Rounds down, so "2h ago" never means
 * three.
 */
export function formatAge(generatedAt: number, now: number = Date.now()): string {
  if (!Number.isFinite(generatedAt) || generatedAt <= 0) return "unknown age";

  const seconds = Math.floor((now - generatedAt) / 1000);
  // Clock skew, or a report written a moment ago on a machine whose clock drifted forward.
  if (seconds < 60) return "just now";

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;

  return `${Math.floor(hours / 24)}d ago`;
}
