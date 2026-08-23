import { useEffect, useRef, useState } from "react";
import { formatPercent } from "../../core/format/percent";
import { formatPrice } from "../../core/format/price";
import { useNow } from "../../core/hooks/useNow";
import { useAlertsStore } from "../../core/store/alerts";
import { useSettingsStore } from "../../core/store/settings";
import { useTickersStore } from "../../core/store/tickers";
import { useUiStore } from "../../core/store/ui";
import { STALE_AFTER_MS, type TickerSnapshot } from "../../types/market";

interface Props {
  symbol: string;
  isPinned: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onEditAlert: () => void;
  onTogglePin: () => void;
}

function displayPrice(ticker: TickerSnapshot | undefined, fiat: string | null, rate: number | null): string {
  if (!ticker) return "···";
  if (fiat && rate) return formatPrice(ticker.price * rate);
  return formatPrice(ticker.price);
}

export function TickerRow({ symbol, isPinned, onToggle, onRemove, onEditAlert, onTogglePin }: Props) {
  const ticker = useTickersStore((s) => s.bySymbol[symbol]);
  const fiat = useSettingsStore((s) => s.settings?.display.fiat ?? null);
  const fxRate = useUiStore((s) => s.fxRate?.rate ?? null);
  const hasAlert = useAlertsStore((s) => s.forSymbol(symbol).some((a) => a.enabled));

  const [flash, setFlash] = useState<"up" | "down" | null>(null);
  const prevPrice = useRef<number | undefined>(ticker?.price);

  useEffect(() => {
    if (ticker && prevPrice.current !== undefined && ticker.price !== prevPrice.current) {
      setFlash(ticker.price > prevPrice.current ? "up" : "down");
      const timeout = setTimeout(() => setFlash(null), 200);
      prevPrice.current = ticker.price;
      return () => clearTimeout(timeout);
    }
    prevPrice.current = ticker?.price;
  }, [ticker?.price]);

  const percent = ticker?.percent24h ?? 0;
  const flashClass = flash === "up" ? "ticker-row--tick-up" : flash === "down" ? "ticker-row--tick-down" : "";

  // A halted pair keeps its last traded price forever: the number stops changing but nothing
  // about it says so. Age it against the wall clock instead of trusting that a fresh tick
  // would have arrived.
  const now = useNow();
  const isStale = !!ticker && now - ticker.eventTime > STALE_AFTER_MS;
  const backupSource = ticker && ticker.source !== "binance" ? ticker.source : null;

  return (
    <div className={`ticker-row ${flashClass} ${isStale ? "ticker-row--stale" : ""}`}>
      <div className="ticker-row__main" onClick={onToggle}>
        <span className="ticker-row__symbol">{symbol}</span>
        {isStale && (
          <span
            className="ticker-row__badge ticker-row__badge--stale"
            title="No live quote — this is the last price the pair traded at"
          >
            STALE
          </span>
        )}
        {!isStale && backupSource && (
          <span className="ticker-row__badge" title={`Price from ${backupSource} — Binance has no live quote for this pair (other watchlist pairs are unaffected)`}>
            {backupSource.toUpperCase()}
          </span>
        )}
        <span className="ticker-row__price mono-nums">{displayPrice(ticker, fiat, fxRate)}</span>
        <span
          className={`ticker-row__percent mono-nums ${
            percent > 0 ? "ticker-row__percent--up" : percent < 0 ? "ticker-row__percent--down" : ""
          }`}
        >
          {formatPercent(percent)}
        </span>
        {hasAlert && <span className="ticker-row__alert-dot" />}
      </div>
      <div className="ticker-row__actions">
        <button
          className={`icon-btn ${isPinned ? "icon-btn--active" : ""}`}
          title={isPinned ? "Unpin from pill" : "Show this in the pill"}
          onClick={onTogglePin}
        >
          {isPinned ? "★" : "☆"}
        </button>
        <button className="icon-btn" title="Alert" onClick={onEditAlert}>
          🔔
        </button>
        <button className="icon-btn" title="Remove" onClick={onRemove}>
          ×
        </button>
      </div>
    </div>
  );
}
