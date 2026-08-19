import { useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useFuturesStore } from "../../core/store/futures";
import type { VenueMode } from "../../types/futures";
import { ApiKeysForm } from "./ApiKeysForm";
import { PositionTable } from "./PositionTable";
import { useFutures } from "./hooks/useFutures";

function formatMoney(value: number): string {
  return Number.isFinite(value) ? value.toFixed(2) : "--";
}

/**
 * Futures tab inside the 380px panel: account totals, open positions, and the setup form when
 * there is nothing connected yet.
 *
 * Deliberately read-only. Orders live in the standalone window, where there is room to show
 * what is about to be sent before it is sent.
 */
export function FuturesPanel() {
  const { state } = useFutures();
  const setEnabled = useFuturesStore((s) => s.setEnabled);
  const setVenue = useFuturesStore((s) => s.setVenue);
  const refresh = useFuturesStore((s) => s.refresh);
  const [setupOpen, setSetupOpen] = useState(false);

  if (!state) {
    return (
      <div className="panel__body">
        <div className="onboarding">
          <span>Loading…</span>
        </div>
      </div>
    );
  }

  if (state.status === "off") {
    return (
      <div className="panel__body">
        <div className="onboarding">
          <span>Futures account is off</span>
          <button className="onboarding__cta" onClick={() => void setEnabled(true)}>
            Connect an account
          </button>
          <span className="settings-panel__hint">
            Reads positions and balance over a signed API connection. Keys are stored in Windows
            Credential Manager, never in the app's settings file.
          </span>
        </div>
      </div>
    );
  }

  const account = state.snapshot?.account;
  const positions = state.snapshot?.positions ?? [];
  const needsKey = state.status === "nokey";

  return (
    <div className="panel__body futures">
      <div className="futures__toolbar">
        <select value={state.mode} onChange={(e) => void setVenue(e.target.value as VenueMode)}>
          <option value="mainnet">Mainnet</option>
          <option value="testnet">Testnet</option>
        </select>
        <span className={`futures__status futures__status--${state.status}`}>{state.status}</span>
        <div className="futures__toolbar-spacer" />
        <button className="icon-btn" title="Refresh now" onClick={() => void refresh()}>
          ↻
        </button>
        <button
          className={`icon-btn ${setupOpen ? "icon-btn--active" : ""}`}
          title="API keys"
          onClick={() => setSetupOpen((open) => !open)}
        >
          🔑
        </button>
        <button
          className="icon-btn"
          title="Open the full futures window"
          onClick={() => void commands.openFuturesWindow()}
        >
          ⤢
        </button>
      </div>

      {state.mode === "testnet" && <div className="futures__testnet-banner">TESTNET</div>}

      {(needsKey || setupOpen) && (
        <>
          <ApiKeysForm venue={state.mode} />
          <button className="referral-panel__disclosure" onClick={() => void setEnabled(false)}>
            Disconnect and switch the feature off
          </button>
        </>
      )}

      {state.status === "error" && state.message && (
        <div className="referral-panel__error">{state.message}</div>
      )}

      {account && (
        <div className="futures__account mono-nums">
          <div>
            <em>margin balance</em> {formatMoney(account.totalMarginBalance)}
          </div>
          <div>
            <em>available</em> {formatMoney(account.availableBalance)}
          </div>
          <div className={account.totalUnrealizedPnl >= 0 ? "pos--up" : "pos--down"}>
            <em>unrealized</em> {formatMoney(account.totalUnrealizedPnl)}
          </div>
        </div>
      )}

      {!needsKey && <PositionTable positions={positions} />}
    </div>
  );
}
