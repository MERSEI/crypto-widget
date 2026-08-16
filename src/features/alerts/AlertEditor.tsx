import { useState } from "react";
import { useAlertsStore } from "../../core/store/alerts";
import type { Alert, AlertKind } from "../../types/settings";
import { AlertsList } from "./AlertsList";

interface Props {
  symbol: string;
  onClose: () => void;
}

export function AlertEditor({ symbol, onClose }: Props) {
  const upsert = useAlertsStore((s) => s.upsert);
  const [kind, setKind] = useState<AlertKind>("price_above");
  const [value, setValue] = useState("");
  const [windowMinutes, setWindowMinutes] = useState("15");
  const [cooldownMin, setCooldownMin] = useState("15");
  const [once, setOnce] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = () => {
    const numericValue = parseFloat(value);
    if (!Number.isFinite(numericValue) || numericValue <= 0) {
      setError("Enter a valid positive value");
      return;
    }
    const alert: Alert = {
      id: crypto.randomUUID(),
      symbol,
      kind,
      value: numericValue,
      windowMinutes: kind === "spike" ? parseInt(windowMinutes, 10) || 15 : null,
      cooldownMin: parseInt(cooldownMin, 10) || 15,
      once,
      enabled: true,
      lastFiredAt: null,
    };
    void upsert(alert).then(() => {
      setValue("");
      setError(null);
    });
  };

  return (
    <div className="alert-overlay">
      <div className="alert-overlay__header">
        <span>{symbol} alerts</span>
        <button className="icon-btn" onClick={onClose}>
          ×
        </button>
      </div>
      <AlertsList symbol={symbol} />
      <div className="alert-editor">
        <div className="alert-editor__row">
          <label>Type</label>
          <select value={kind} onChange={(e) => setKind(e.target.value as AlertKind)}>
            <option value="price_above">Price above</option>
            <option value="price_below">Price below</option>
            <option value="spike">Spike %</option>
          </select>
        </div>
        <div className="alert-editor__row">
          <label>{kind === "spike" ? "Threshold %" : "Price"}</label>
          <input value={value} onChange={(e) => setValue(e.target.value)} placeholder="0.00" />
        </div>
        {kind === "spike" && (
          <div className="alert-editor__row">
            <label>Window (min)</label>
            <input value={windowMinutes} onChange={(e) => setWindowMinutes(e.target.value)} />
          </div>
        )}
        <div className="alert-editor__row">
          <label>Cooldown (min)</label>
          <input value={cooldownMin} onChange={(e) => setCooldownMin(e.target.value)} />
        </div>
        <div className="alert-editor__row">
          <label>Once</label>
          <input type="checkbox" checked={once} onChange={(e) => setOnce(e.target.checked)} style={{ flex: "none" }} />
        </div>
        {error && <span style={{ color: "var(--down)", fontSize: 11 }}>{error}</span>}
        <div className="alert-editor__actions">
          <button className="btn" onClick={onClose}>
            Close
          </button>
          <button className="btn btn--primary" onClick={handleSave}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
