import { useEffect, useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useSettingsStore } from "../../core/store/settings";
import type { Candle, ChartType, Timeframe } from "../../types/market";
import { PriceChart } from "./PriceChart";
import { TimeframeBar } from "./TimeframeBar";

const CACHE_TTL_MS = 60_000;
const REFRESH_MS = 60_000;
const klinesCache = new Map<string, { data: Candle[]; fetchedAt: number }>();

async function fetchKlines(symbol: string, interval: Timeframe): Promise<Candle[]> {
  const key = `${symbol}:${interval}`;
  const cached = klinesCache.get(key);
  if (cached && Date.now() - cached.fetchedAt < CACHE_TTL_MS) {
    return cached.data;
  }
  const data = await commands.getKlines(symbol, interval);
  klinesCache.set(key, { data, fetchedAt: Date.now() });
  return data;
}

interface Props {
  symbol: string;
}

export function ChartAccordion({ symbol }: Props) {
  const defaultTimeframe = useSettingsStore((s) => s.settings?.chart.defaultTimeframe ?? "4h");
  const defaultType = useSettingsStore((s) => s.settings?.chart.type ?? "area");
  const [timeframe, setTimeframe] = useState<Timeframe>(defaultTimeframe);
  const [tall, setTall] = useState(false);
  const [candles, setCandles] = useState<Candle[]>([]);
  const [error, setError] = useState<string | null>(null);

  const type: ChartType = tall ? "candlestick" : defaultType;

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      fetchKlines(symbol, timeframe)
        .then((data) => {
          if (!cancelled) {
            setCandles(data);
            setError(null);
          }
        })
        .catch((e) => {
          if (!cancelled) setError(String(e));
        });
    };
    load();
    const interval = setInterval(load, REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [symbol, timeframe]);

  return (
    <div className="chart-accordion" style={{ height: tall ? 300 : 150 }}>
      <div className="chart-accordion__toolbar">
        <TimeframeBar value={timeframe} onChange={setTimeframe} />
        <button className="icon-btn" title="Expand" onClick={() => setTall((v) => !v)}>
          ⛶
        </button>
      </div>
      {error ? (
        <div className="chart-accordion__error">Chart unavailable: {error}</div>
      ) : (
        <PriceChart candles={candles} type={type} />
      )}
    </div>
  );
}
