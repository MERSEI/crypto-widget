import { useState } from "react";
import { formatPercent } from "../../core/format/percent";
import { formatPrice } from "../../core/format/price";
import { useFuturesStore } from "../../core/store/futures";
import type { Position } from "../../types/futures";

interface Props {
  positions: Position[];
  /** `compact` fits the 380px panel; `full` is the wider standalone window. */
  variant?: "compact" | "full";
  /** Shows Close/Reduce on each row. Only ever true for `variant="full"` in the standalone
   *  window — `FuturesHub` would refuse the request anyway on mainnet, but the button not being
   *  there is a clearer signal than a click that always fails. */
  tradingAllowed?: boolean;
}

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

type RowAction = { symbol: string; mode: "close" | "reduce"; quantity: string } | null;

function signClass(value: number): string {
  if (value > 0) return "pos--up";
  if (value < 0) return "pos--down";
  return "";
}

/** PnL is money, not a price: two decimals regardless of magnitude. */
function formatPnl(value: number): string {
  if (!Number.isFinite(value)) return "--";
  const normalized = value === 0 ? 0 : value;
  const sign = normalized > 0 ? "+" : "";
  return `${sign}${normalized.toFixed(2)}`;
}

function formatAmount(amount: number): string {
  const abs = Math.abs(amount);
  // Contract sizes span 0.001 BTC to thousands of DOGE; trailing zeros just add noise.
  const decimals = abs >= 100 ? 0 : abs >= 1 ? 2 : 4;
  return abs.toFixed(decimals);
}

export function PositionTable({ positions, variant = "compact", tradingAllowed = false }: Props) {
  const closePosition = useFuturesStore((s) => s.closePosition);
  const [action, setAction] = useState<RowAction>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (positions.length === 0) {
    return <div className="futures__empty">No open positions</div>;
  }

  async function submitAction(position: Position) {
    if (!action || action.symbol !== position.symbol) return;
    // Closing a long sells; closing a short buys — the opposite of how the position was opened.
    const closingSide = position.positionAmt > 0 ? "sell" : "buy";
    const quantity = action.mode === "reduce" ? Number(action.quantity) : null;
    setBusy(true);
    setError(null);
    try {
      await closePosition(position.symbol, closingSide, quantity);
      setAction(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={`position-table position-table--${variant}`}>
      {positions.map((position) => {
        const side = position.positionAmt > 0 ? "LONG" : "SHORT";
        const rowAction = action?.symbol === position.symbol ? action : null;
        return (
          <div className="position-row" key={`${position.symbol}-${position.positionSide}`}>
            <div className="position-row__head">
              <span className="position-row__symbol">{position.symbol}</span>
              <span className={`position-row__side position-row__side--${side.toLowerCase()}`}>{side}</span>
              <span className="mono-nums position-row__amt">{formatAmount(position.positionAmt)}</span>
              {position.leverage !== null && (
                <span className="position-row__leverage">{position.leverage}x</span>
              )}
              <span className={`mono-nums position-row__pnl ${signClass(position.unrealizedPnl)}`}>
                {formatPnl(position.unrealizedPnl)}
              </span>
            </div>

            <div className="position-row__detail mono-nums">
              <span>
                <em>entry</em> {formatPrice(position.entryPrice)}
              </span>
              <span>
                <em>mark</em> {formatPrice(position.markPrice)}
              </span>
              <span className={signClass(position.priceChangePercent)}>
                <em>chg</em> {formatPercent(position.priceChangePercent)}
              </span>
              {position.roePercent !== null && (
                <span className={signClass(position.roePercent)}>
                  <em>roe</em> {formatPercent(position.roePercent)}
                </span>
              )}
              {position.liquidationPrice !== null && (
                <span className="position-row__liq">
                  <em>liq</em> {formatPrice(position.liquidationPrice)}
                </span>
              )}
              {variant === "full" && (
                <>
                  <span>
                    <em>notional</em> {formatPrice(position.notional)}
                  </span>
                  <span>
                    <em>margin</em> {position.marginType}
                  </span>
                </>
              )}
            </div>

            {variant === "full" && tradingAllowed && (
              <div className="position-row__actions">
                {rowAction ? (
                  <>
                    {rowAction.mode === "reduce" && (
                      <input
                        className="wallet__input mono-nums"
                        style={{ maxWidth: "6rem" }}
                        inputMode="decimal"
                        placeholder="qty"
                        value={rowAction.quantity}
                        onChange={(e) =>
                          setAction({ ...rowAction, quantity: e.target.value })
                        }
                      />
                    )}
                    <button
                      className="btn btn--danger"
                      disabled={
                        busy || (rowAction.mode === "reduce" && !(Number(rowAction.quantity) > 0))
                      }
                      onClick={() => void submitAction(position)}
                    >
                      Confirm {rowAction.mode}
                    </button>
                    <button
                      className="btn"
                      disabled={busy}
                      onClick={() => {
                        setError(null);
                        setAction(null);
                      }}
                    >
                      Cancel
                    </button>
                    {error && <span className="wallet__error">{error}</span>}
                  </>
                ) : (
                  <>
                    <button
                      className="btn"
                      onClick={() => {
                        setError(null);
                        setAction({ symbol: position.symbol, mode: "reduce", quantity: "" });
                      }}
                    >
                      Reduce
                    </button>
                    <button
                      className="btn btn--danger"
                      onClick={() => {
                        setError(null);
                        setAction({ symbol: position.symbol, mode: "close", quantity: "" });
                      }}
                    >
                      Close
                    </button>
                  </>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
