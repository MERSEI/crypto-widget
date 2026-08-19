import { formatPercent } from "../../core/format/percent";
import { formatPrice } from "../../core/format/price";
import type { Position } from "../../types/futures";

interface Props {
  positions: Position[];
  /** `compact` fits the 380px panel; `full` is the wider standalone window. */
  variant?: "compact" | "full";
}

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

export function PositionTable({ positions, variant = "compact" }: Props) {
  if (positions.length === 0) {
    return <div className="futures__empty">No open positions</div>;
  }

  return (
    <div className={`position-table position-table--${variant}`}>
      {positions.map((position) => {
        const side = position.positionAmt > 0 ? "LONG" : "SHORT";
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
          </div>
        );
      })}
    </div>
  );
}
