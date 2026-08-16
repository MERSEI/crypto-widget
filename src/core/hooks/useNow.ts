import { useEffect, useState } from "react";

/**
 * Re-renders the caller on a timer so time-derived state stays honest without new data.
 *
 * A row only re-renders when its ticker changes — which is exactly what stops happening when
 * a pair is halted. Without a clock of its own, the "this price is stale" badge would wait
 * for a tick that never arrives.
 */
export function useNow(intervalMs = 5000): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  return now;
}
