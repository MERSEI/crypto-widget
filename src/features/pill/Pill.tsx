import { formatPercent } from "../../core/format/percent";
import { useWatchlistStore } from "../../core/store/watchlist";
import { useTickersStore } from "../../core/store/tickers";
import { useUiStore } from "../../core/store/ui";
import { commands } from "../../core/ipc/commands";
import { usePillDrag } from "./usePillDrag";

export function Pill() {
  const items = useWatchlistStore((s) => s.items);
  const topSymbol = items[0]?.symbol;
  const topTicker = useTickersStore((s) => (topSymbol ? s.bySymbol[topSymbol] : undefined));
  const connectionState = useUiStore((s) => s.connection.state);

  const handleClick = () => {
    void commands.toggleExpand().then((expanded) => useUiStore.getState().setExpanded(expanded));
  };
  const { onPointerDown } = usePillDrag(handleClick);

  const delta = topTicker?.percent24h ?? 0;
  const label = topSymbol ? topSymbol.replace(/USDT$/, "") : "ADD";

  return (
    <div className="pill" onPointerDown={onPointerDown} title="Click to open · drag to move">
      <span className={`pill__connection pill__connection--${connectionState}`} />
      <span className="pill__ticker mono-nums">{label}</span>
      {topTicker && <span className="pill__delta mono-nums">{formatPercent(delta)}</span>}
      <span className={`pill__dot ${delta > 0 ? "pill__dot--up" : delta < 0 ? "pill__dot--down" : ""}`} />
    </div>
  );
}
