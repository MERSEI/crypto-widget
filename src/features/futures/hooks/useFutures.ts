import { useEffect } from "react";
import { onFutures } from "../../../core/ipc/events";
import { useFuturesStore } from "../../../core/store/futures";

/**
 * Hydrates the futures store and keeps it subscribed to the backend's `futures` event.
 *
 * Safe to call from more than one component — including from a second window — because the
 * backend is the only writer and every listener receives the same state object.
 */
export function useFutures() {
  const hydrate = useFuturesStore((s) => s.hydrate);
  const apply = useFuturesStore((s) => s.apply);

  useEffect(() => {
    void hydrate();
    const unlisten = onFutures(apply);
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [hydrate, apply]);

  return {
    state: useFuturesStore((s) => s.state),
    keys: useFuturesStore((s) => s.keys),
  };
}
