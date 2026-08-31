import { useState } from "react";
import { useInsightsStore } from "../../core/store/insights";

/**
 * The key and the cost levers.
 *
 * Lives inside the AI tab rather than in the app's settings overlay, next to the thing it
 * governs — the same choice the futures and referral panels make. The key is held in component
 * state until it is submitted and never comes back from Rust: what returns is a masked hint,
 * which is why there is no "reveal" affordance.
 */
export function InsightsSettingsForm({ onDone }: { onDone?: () => void }) {
  const state = useInsightsStore((s) => s.state);
  const saveKey = useInsightsStore((s) => s.saveKey);
  const clearKey = useInsightsStore((s) => s.clearKey);
  const saveSettings = useInsightsStore((s) => s.saveSettings);

  const [draftKey, setDraftKey] = useState("");
  const [busy, setBusy] = useState(false);

  if (!state) return null;
  const { settings, key, models } = state;

  const handleSaveKey = async () => {
    setBusy(true);
    try {
      await saveKey(draftKey);
      setDraftKey("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="insights-setup">
      {key.present ? (
        <div className="insights-setup__stored">
          <span className="mono-nums">key {key.maskedKey}</span>
          <button className="btn" disabled={busy} onClick={() => void clearKey()}>
            Remove
          </button>
        </div>
      ) : (
        <div className="referral-panel__field">
          <label htmlFor="anthropic-key">Anthropic API key</label>
          <input
            id="anthropic-key"
            type="password"
            value={draftKey}
            placeholder="sk-ant-…"
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setDraftKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && draftKey.trim()) void handleSaveKey();
            }}
          />
          <div className="settings-panel__hint">
            Stored in the Windows Credential Manager, never in <code>settings.json</code>. Calls
            go straight from this machine to api.anthropic.com and are billed to your key.
          </div>
          <button
            className="btn btn--primary"
            disabled={busy || !draftKey.trim()}
            onClick={() => void handleSaveKey()}
          >
            Save key
          </button>
        </div>
      )}

      <div className="settings-panel__row">
        <span className="settings-panel__label">Model</span>
        <select
          className="select-input"
          value={settings.model}
          onChange={(e) => void saveSettings({ model: e.target.value })}
        >
          {models.map((model) => (
            <option key={model} value={model}>
              {model}
            </option>
          ))}
        </select>
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Answer language</span>
        <select
          className="select-input"
          value={settings.language}
          onChange={(e) => void saveSettings({ language: e.target.value })}
        >
          <option value="en">English</option>
          <option value="ru">Русский</option>
        </select>
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Keep answers for</span>
        <select
          className="select-input"
          value={settings.cacheTtlMin}
          onChange={(e) => void saveSettings({ cacheTtlMin: Number(e.target.value) })}
        >
          <option value={15}>15 min</option>
          <option value={60}>1 hour</option>
          <option value={120}>2 hours</option>
          <option value={480}>8 hours</option>
          <option value={1440}>1 day</option>
        </select>
      </div>

      <div className="settings-panel__row">
        <span className="settings-panel__label">Web searches per call</span>
        <select
          className="select-input"
          value={settings.maxSearches}
          onChange={(e) => void saveSettings({ maxSearches: Number(e.target.value) })}
        >
          <option value={3}>3 — cheap</option>
          <option value={6}>6 — balanced</option>
          <option value={10}>10 — thorough</option>
        </select>
      </div>

      <div className="settings-panel__hint">
        Every report costs tokens plus one charge per web search. Answers are cached for the
        window above; only the refresh button pays again.
      </div>

      {onDone && (
        <button className="btn" onClick={onDone}>
          Done
        </button>
      )}
    </div>
  );
}
