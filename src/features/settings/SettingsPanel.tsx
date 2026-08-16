import { useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useSettingsStore } from "../../core/store/settings";
import { useUiStore } from "../../core/store/ui";
import type { Timeframe } from "../../types/market";

interface Props {
  onClose: () => void;
}

function Toggle({ on, onChange }: { on: boolean; onChange: (next: boolean) => void }) {
  return (
    <div className={`toggle ${on ? "toggle--on" : ""}`} onClick={() => onChange(!on)}>
      <div className="toggle__knob" />
    </div>
  );
}

export function SettingsPanel({ onClose }: Props) {
  const settings = useSettingsStore((s) => s.settings);
  const setDisplay = useSettingsStore((s) => s.setDisplay);
  const setChart = useSettingsStore((s) => s.setChart);
  const setNotifications = useSettingsStore((s) => s.setNotifications);
  const setAutostart = useSettingsStore((s) => s.setAutostart);
  const setFxRate = useUiStore((s) => s.setFxRate);
  // Declared before the early return: hooks can't sit behind a conditional.
  const [testResult, setTestResult] = useState<string | null>(null);

  if (!settings) return null;

  const czkEnabled = settings.display.fiat === "CZK";

  const handleToggleCzk = async (next: boolean) => {
    await setDisplay(settings.display.quote, next ? "CZK" : null);
    if (next) {
      const rate = await commands.getFxRate();
      setFxRate(rate);
    }
  };

  const handleTest = async () => {
    try {
      setTestResult(await commands.sendTestNotification());
    } catch (e) {
      setTestResult(String(e));
    }
  };

  return (
    <div className="alert-overlay">
      <div className="alert-overlay__header">
        <span>Settings</span>
        <button className="icon-btn" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Show CZK</span>
        <Toggle on={czkEnabled} onChange={(v) => void handleToggleCzk(v)} />
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Default timeframe</span>
        <select
          value={settings.chart.defaultTimeframe}
          onChange={(e) => void setChart({ ...settings.chart, defaultTimeframe: e.target.value as Timeframe })}
        >
          <option value="1h">1h</option>
          <option value="4h">4h</option>
          <option value="1d">1D</option>
          <option value="1w">1W</option>
        </select>
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Default chart type</span>
        <select
          value={settings.chart.type}
          onChange={(e) => void setChart({ ...settings.chart, type: e.target.value as "area" | "candlestick" })}
        >
          <option value="area">Area</option>
          <option value="candlestick">Candles</option>
        </select>
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Alert toasts</span>
        <Toggle
          on={settings.notifications.toast}
          onChange={(v) => void setNotifications(v, settings.notifications.sound)}
        />
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Alert sound</span>
        <Toggle
          on={settings.notifications.sound}
          onChange={(v) => void setNotifications(settings.notifications.toast, v)}
        />
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Test notification</span>
        <button className="icon-btn" title="Send a sample BTC spike toast" onClick={() => void handleTest()}>
          Send
        </button>
      </div>
      {testResult && <div className="settings-panel__hint">{testResult}</div>}

      <div className="settings-panel__row">
        <span className="settings-panel__label">Launch at startup</span>
        <Toggle on={settings.autostart} onChange={(v) => void setAutostart(v)} />
      </div>
    </div>
  );
}
