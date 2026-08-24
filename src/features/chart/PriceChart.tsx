import { useEffect, useRef } from "react";
import {
  AreaSeries,
  CandlestickSeries,
  createChart,
  type IChartApi,
  type ISeriesApi,
  type Time,
} from "lightweight-charts";
import type { Candle, ChartType } from "../../types/market";

interface Props {
  candles: Candle[];
  type: ChartType;
}

export function PriceChart({ candles, type }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Area"> | ISeriesApi<"Candlestick"> | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: {
        background: { color: "transparent" },
        textColor: "#9aa3a8",
        fontFamily: "Cascadia Mono, JetBrains Mono, Consolas, monospace",
        fontSize: 10,
      },
      grid: { vertLines: { visible: false }, horzLines: { color: "#1a1d20" } },
      rightPriceScale: { borderVisible: false },
      timeScale: { borderVisible: false },
      autoSize: true,
    });
    chartRef.current = chart;
    return () => {
      chart.remove();
      chartRef.current = null;
      seriesRef.current = null;
    };
  }, []);

  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;

    if (seriesRef.current) {
      chart.removeSeries(seriesRef.current);
      seriesRef.current = null;
    }

    if (type === "area") {
      const series = chart.addSeries(AreaSeries, {
        lineColor: "#1fd67a",
        topColor: "rgba(31,214,122,0.28)",
        bottomColor: "rgba(31,214,122,0.02)",
        lineWidth: 1,
      });
      series.setData(candles.map((c) => ({ time: c.time as Time, value: c.close })));
      seriesRef.current = series;
    } else {
      const series = chart.addSeries(CandlestickSeries, {
        upColor: "#2fe98d",
        downColor: "#ff6161",
        borderVisible: false,
        wickUpColor: "#2fe98d",
        wickDownColor: "#ff6161",
      });
      series.setData(
        candles.map((c) => ({ time: c.time as Time, open: c.open, high: c.high, low: c.low, close: c.close })),
      );
      seriesRef.current = series;
    }

    chart.timeScale().fitContent();
  }, [candles, type]);

  return <div ref={containerRef} className="chart-accordion__canvas" />;
}
