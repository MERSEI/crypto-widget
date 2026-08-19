import { useFuturesStore } from "../../core/store/futures";
import type { VenueMode } from "../../types/futures";
import { ApiKeysForm } from "./ApiKeysForm";
import { PositionTable } from "./PositionTable";
import { useFutures } from "./hooks/useFutures";

function formatMoney(value: number): string {
  return Number.isFinite(value) ? value.toFixed(2) : "--";
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <div className="futures-window__stat">
      <span className="futures-window__stat-label">{label}</span>
      <span className={`mono-nums futures-window__stat-value ${tone ?? ""}`}>{value}</span>
    </div>
  );
}

/**
 * The standalone futures terminal — the same state as the panel tab, with room to show it.
 *
 * This is a separate top-level root rather than a route inside `App`: the pill window is
 * transparent, undecorated and 140×30, and none of that suits a table. Both roots read the same
 * store, and the backend broadcasts `futures` to every window, so they stay in step without
 * talking to each other.
 */
export function FuturesWindow() {
  const { state } = useFutures();
  const setVenue = useFuturesStore((s) => s.setVenue);
  const refresh = useFuturesStore((s) => s.refresh);

  if (!state) {
    return (
      <div className="futures-window">
        <div className="onboarding">
          <span>Loading…</span>
        </div>
      </div>
    );
  }

  const account = state.snapshot?.account;
  const positions = state.snapshot?.positions ?? [];
  const updatedAt = state.snapshot?.updatedAt;

  return (
    <div className="futures-window">
      <header className="futures-window__header">
        <span className="panel__title">FUTURES</span>
        <select value={state.mode} onChange={(e) => void setVenue(e.target.value as VenueMode)}>
          <option value="mainnet">Mainnet</option>
          <option value="testnet">Testnet</option>
        </select>
        {state.mode === "testnet" ? (
          <span className="futures__testnet-banner">TESTNET</span>
        ) : (
          <span className="futures-window__readonly" title="Orders are only ever sent to the testnet">
            READ-ONLY
          </span>
        )}
        <span className={`futures__status futures__status--${state.status}`}>{state.status}</span>
        <div className="futures__toolbar-spacer" />
        {updatedAt && (
          <span className="futures-window__updated mono-nums">
            {new Date(updatedAt).toTimeString().slice(0, 8)}
          </span>
        )}
        <button className="icon-btn" title="Refresh now" onClick={() => void refresh()}>
          ↻
        </button>
      </header>

      {state.status === "error" && state.message && (
        <div className="referral-panel__error">{state.message}</div>
      )}

      {(state.status === "nokey" || state.status === "off") && (
        <div className="futures-window__setup">
          <ApiKeysForm venue={state.mode} />
        </div>
      )}

      {account && (
        <div className="futures-window__stats">
          <Stat label="Wallet balance" value={formatMoney(account.totalWalletBalance)} />
          <Stat label="Margin balance" value={formatMoney(account.totalMarginBalance)} />
          <Stat label="Available" value={formatMoney(account.availableBalance)} />
          <Stat
            label="Unrealized PnL"
            value={formatMoney(account.totalUnrealizedPnl)}
            tone={account.totalUnrealizedPnl >= 0 ? "pos--up" : "pos--down"}
          />
        </div>
      )}

      <section className="futures-window__positions">
        <h2 className="futures-window__section-title">Open positions</h2>
        <PositionTable positions={positions} variant="full" />
      </section>
    </div>
  );
}
