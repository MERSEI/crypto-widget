import { useMemo } from "react";
import { useAlertsStore } from "../../core/store/alerts";
import type { Alert } from "../../types/settings";

function describe(alert: Alert): string {
  switch (alert.kind) {
    case "price_above":
      return `price above ${alert.value}`;
    case "price_below":
      return `price below ${alert.value}`;
    case "spike":
      return `spike ${alert.value}% / ${alert.windowMinutes ?? 15}m`;
  }
}

interface Props {
  symbol: string;
}

export function AlertsList({ symbol }: Props) {
  // Selecting `s.forSymbol(symbol)` returned a freshly filtered array on every call, and
  // zustand v5 feeds the selector straight to `useSyncExternalStore` with no equality shim —
  // React sees a new snapshot each render and throws "The result of getSnapshot should be
  // cached to avoid an infinite loop", which tears down the whole tree. Select the stable
  // `items` reference and filter here instead.
  const items = useAlertsStore((s) => s.items);
  const alerts = useMemo(() => items.filter((a) => a.symbol === symbol), [items, symbol]);
  const remove = useAlertsStore((s) => s.remove);

  if (alerts.length === 0) {
    return null;
  }

  return (
    <div className="alerts-list">
      {alerts.map((alert) => (
        <div key={alert.id} className={`alerts-list__row ${alert.enabled ? "" : "alerts-list__row--disabled"}`}>
          <span>{describe(alert)}</span>
          <button className="icon-btn" onClick={() => void remove(alert.id)}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
