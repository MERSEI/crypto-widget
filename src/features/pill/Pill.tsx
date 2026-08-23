import { useRef } from "react";
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

  // A second click landing before the first `toggleExpand()` resolves used to fire a second,
  // overlapping resize on the Rust side. The lock there now makes that safe, but there is still
  // no reason to send it — the in-flight request already carries whatever the latest click meant.
  const toggling = useRef(false);
  const handleClick = () => {
    if (toggling.current) return;
    toggling.current = true;
    void commands
      .toggleExpand()
      .then((expanded) => useUiStore.getState().setExpanded(expanded))
      .finally(() => {
        toggling.current = false;
      });
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
