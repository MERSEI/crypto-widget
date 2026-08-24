import { useState } from "react";
import { useFuturesStore } from "../../core/store/futures";

function when(timestamp: number): string {
  if (!timestamp) return "—";
  const date = new Date(timestamp);
  return `${date.toLocaleDateString()} ${date.toTimeString().slice(0, 5)}`;
}

/**
 * Order history for one symbol at a time — Binance's futures API has no "every symbol at once"
 * endpoint for this, so the tab always names one before it has anything to show.
 */
export function OrderHistory() {
  const symbol = useFuturesStore((s) => s.orderHistorySymbol);
  const orders = useFuturesStore((s) => s.orderHistory);
  const loading = useFuturesStore((s) => s.orderHistoryLoading);
  const error = useFuturesStore((s) => s.orderHistoryError);
  const loadOrderHistory = useFuturesStore((s) => s.loadOrderHistory);

  const [input, setInput] = useState(symbol);

  return (
    <section className="wallet-settings__group">
      <h3 className="wallet-settings__group-title">Order history</h3>
      <div className="wallet-field wallet-field--inline">
        <input
          className="wallet__input mono-nums"
          placeholder="BTCUSDT"
          spellCheck={false}
          autoComplete="off"
          value={input}
          onChange={(e) => setInput(e.target.value)}
        />
        <button
          className="btn"
          disabled={loading || input.trim().length === 0}
          onClick={() => void loadOrderHistory(input.trim().toUpperCase())}
        >
          Load
        </button>
      </div>

      {error && <div className="wallet__error">{error}</div>}

      {!error && !loading && symbol && orders.length === 0 && (
        <p className="wallet__empty">No orders for {symbol}.</p>
      )}

      {orders.length > 0 && (
        <table className="wallet-history__table">
          <tbody>
            {orders.map((order) => (
              <tr key={order.orderId}>
                <td className="wallet-history__when mono-nums">{when(order.time)}</td>
                <td
                  className={`wallet-history__direction wallet-history__direction--${
                    order.side === "BUY" ? "in" : "out"
                  }`}
                >
                  {order.side}
                </td>
                <td className="mono-nums">{order.orderType}</td>
                <td className="mono-nums" title={String(order.origQty)}>
                  {order.executedQty}/{order.origQty}
                </td>
                <td className="mono-nums">{order.avgPrice || order.price || "—"}</td>
                <td className="wallet-history__state">{order.status.toLowerCase()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
