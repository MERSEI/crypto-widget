import { useEffect } from "react";
import { useWalletStore } from "../../core/store/wallet";

function when(timestamp: number): string {
  const date = new Date(timestamp);
  return `${date.toLocaleDateString()} ${date.toTimeString().slice(0, 5)}`;
}

/** `0x1234…cdef` — enough to recognise an address, short enough to sit in a table row. */
function shortAddress(value: string | null): string {
  if (!value) return "contract creation";
  return value.length > 12 ? `${value.slice(0, 6)}…${value.slice(-4)}` : value;
}

/**
 * Transfers in and out of the active account, newest first.
 *
 * Loaded on mount rather than with the balances: it needs an Etherscan key, and a wallet
 * without one is perfectly usable — the missing key shows up as one message here instead of
 * breaking the whole screen.
 */
export function HistoryList() {
  const history = useWalletStore((s) => s.history);
  const loading = useWalletStore((s) => s.historyLoading);
  const error = useWalletStore((s) => s.historyError);
  const refreshHistory = useWalletStore((s) => s.refreshHistory);

  useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  return (
    <section className="wallet-history">
      <header className="wallet-section__header">
        <h2 className="wallet-section__title">History</h2>
        <div className="futures__toolbar-spacer" />
        <button
          className="icon-btn"
          title="Refresh history"
          disabled={loading}
          onClick={() => void refreshHistory()}
        >
          ↻
        </button>
      </header>

      {error && <div className="wallet__error">{error}</div>}

      {history.length === 0 && !loading && !error && (
        <p className="wallet__empty">No transfers for this account.</p>
      )}

      {history.length > 0 && (
        <table className="wallet-history__table">
          <tbody>
            {history.map((tx) => (
              <tr
                key={`${tx.hash}-${tx.symbol}-${tx.timestamp}`}
                className={tx.failed ? "wallet-history__row--failed" : ""}
              >
                <td className="wallet-history__when mono-nums">{when(tx.timestamp)}</td>
                <td
                  className={`wallet-history__direction wallet-history__direction--${tx.direction}`}
                >
                  {tx.direction === "in" ? "IN" : "OUT"}
                </td>
                <td className="wallet-history__amount mono-nums" title={tx.amount}>
                  {tx.amount} {tx.symbol}
                </td>
                <td className="wallet-history__peer mono-nums" title={tx.to ?? tx.from}>
                  {tx.direction === "in" ? shortAddress(tx.from) : shortAddress(tx.to)}
                </td>
                <td className="wallet-history__state">
                  {/* A reverted transaction is kept in the list because it happened and it cost
                      gas — but it moved nothing, so it must never read as a completed transfer. */}
                  {tx.failed ? "failed" : ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
