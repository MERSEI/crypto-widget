import { useEffect, useRef, useState } from "react";
import { formatPercent } from "../../core/format/percent";
import { formatPrice } from "../../core/format/price";
import { useAlertsStore } from "../../core/store/alerts";
import { useSettingsStore } from "../../core/store/settings";
import { useTickersStore } from "../../core/store/tickers";
import { useUiStore } from "../../core/store/ui";
import type { TickerSnapshot } from "../../types/market";

interface Props {
  symbol: string;
  onToggle: () => void;
  onRemove: () => void;
  onEditAlert: () => void;
}

function displayPrice(ticker: TickerSnapshot | undefined, fiat: string | null, rate: number | null): string {
  if (!ticker) return "···";
  if (fiat && rate) return formatPrice(ticker.price * rate);
  return formatPrice(ticker.price);
}

export function TickerRow({ symbol, onToggle, onRemove, onEditAlert }: Props) {
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

  return (
    <div className={`ticker-row ${flashClass}`}>
      <div className="ticker-row__main" onClick={onToggle}>
        <span className="ticker-row__symbol">{symbol}</span>
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
