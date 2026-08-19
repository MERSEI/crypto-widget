import { useState } from "react";
import { useFuturesStore } from "../../core/store/futures";
import type { VenueMode } from "../../types/futures";

/**
 * API key entry.
 *
 * The secret is held in component state only until it is submitted, and the backend never sends
 * one back — what comes back is a masked hint. That is why there is no "show current key"
 * affordance: the app genuinely cannot display it.
 */
export function ApiKeysForm({ venue }: { venue: VenueMode }) {
  const keys = useFuturesStore((s) => s.keys);
  const saveKeys = useFuturesStore((s) => s.saveKeys);
  const clearKeys = useFuturesStore((s) => s.clearKeys);
  const testConnection = useFuturesStore((s) => s.testConnection);
  const testResult = useFuturesStore((s) => s.testResult);

  const [apiKey, setApiKey] = useState("");
  const [apiSecret, setApiSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const stored = venue === "mainnet" ? keys?.mainnet : keys?.testnet;

  const handleSave = async () => {
    setBusy(true);
    setError(null);
    try {
      await saveKeys(venue, apiKey, apiSecret);
      setApiKey("");
      setApiSecret("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleClear = async () => {
    setBusy(true);
    setError(null);
    try {
      await clearKeys(venue);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="api-keys">
      {stored?.present ? (
        <div className="api-keys__stored">
          <span className="mono-nums">key {stored.maskedKey}</span>
          <div className="referral-link__actions">
            <button className="btn" disabled={busy} onClick={() => void testConnection()}>
              Test
            </button>
            <button className="btn" disabled={busy} onClick={() => void handleClear()}>
              Remove
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="referral-panel__field">
            <label htmlFor="futures-key">API key</label>
            <input
              id="futures-key"
              value={apiKey}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setApiKey(e.target.value)}
            />
          </div>
          <div className="referral-panel__field">
            <label htmlFor="futures-secret">API secret</label>
            <input
              id="futures-secret"
              type="password"
              value={apiSecret}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setApiSecret(e.target.value)}
            />
          </div>
          <button
            className="btn btn--primary"
            disabled={busy || !apiKey.trim() || !apiSecret.trim()}
            onClick={() => void handleSave()}
          >
            Save to Windows Credential Manager
          </button>
          <div className="settings-panel__hint">
            {venue === "mainnet"
              ? "Create this key with Enable Reading only — this app never sends an order to mainnet, and a read-only key means a bug here cannot move your money."
              : "Get a testnet key from testnet.binancefuture.com. Orders are only ever placed against this venue."}
          </div>
        </>
      )}
      {testResult && <div className="settings-panel__hint">{testResult}</div>}
      {error && <div className="referral-panel__error">{error}</div>}
    </div>
  );
}
