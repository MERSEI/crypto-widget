import { useEffect, useState } from "react";
import { useUiStore } from "../../core/store/ui";

const LABELS: Record<string, string> = {
  connecting: "CONNECTING",
  live: "BINANCE WS",
  stale: "STALE",
  reconnecting: "RECONNECTING",
  polling: "POLLING",
  offline: "OFFLINE",
};

export function StatusBar() {
  const connection = useUiStore((s) => s.connection);
  const [clock, setClock] = useState(() => new Date());

  useEffect(() => {
    const id = setInterval(() => setClock(new Date()), 1000);
    return () => clearInterval(id);
  }, []);

  const label = LABELS[connection.state] ?? connection.state.toUpperCase();
  const attemptSuffix =
    connection.state === "reconnecting" && connection.attempt > 0 ? `(${connection.attempt})` : "";
  const time = clock.toTimeString().slice(0, 8);

  return (
    <div className={`status-bar status-bar--${connection.state}`}>
      <span className="status-bar__dot" />
      <span className="mono-nums">
        {label}
        {attemptSuffix} · {time}
      </span>
    </div>
  );
}
