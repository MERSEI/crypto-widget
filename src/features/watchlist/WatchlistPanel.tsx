import { useState } from "react";
import { useUiStore } from "../../core/store/ui";
import { useWatchlistStore } from "../../core/store/watchlist";
import { ChartAccordion } from "../chart/ChartAccordion";
import { AlertEditor } from "../alerts/AlertEditor";
import { SearchDialog } from "./SearchDialog";
import { TickerRow } from "./TickerRow";

export function WatchlistPanel() {
  const items = useWatchlistStore((s) => s.items);
  const remove = useWatchlistStore((s) => s.remove);
  const openSymbol = useUiStore((s) => s.openSymbol);
  const toggleOpenSymbol = useUiStore((s) => s.toggleOpenSymbol);
  const searchOpen = useUiStore((s) => s.searchOpen);
  const setSearchOpen = useUiStore((s) => s.setSearchOpen);
  const [alertSymbol, setAlertSymbol] = useState<string | null>(null);

  if (items.length === 0) {
    return (
      <div className="panel__body" style={{ position: "relative" }}>
        <div className="onboarding">
          <span>Watchlist is empty</span>
          <button className="onboarding__cta" onClick={() => setSearchOpen(true)}>
            + Add your first coin
          </button>
        </div>
        {searchOpen && <SearchDialog onClose={() => setSearchOpen(false)} />}
      </div>
    );
  }

  return (
    <div className="panel__body" style={{ position: "relative" }}>
      <div className="watchlist">
        {items.map((item) => (
          <div key={item.symbol}>
            <TickerRow
              symbol={item.symbol}
              onToggle={() => toggleOpenSymbol(item.symbol)}
              onRemove={() => void remove(item.symbol)}
              onEditAlert={() => setAlertSymbol(item.symbol)}
            />
            {openSymbol === item.symbol && <ChartAccordion symbol={item.symbol} />}
          </div>
        ))}
      </div>
      <button className="onboarding__cta" style={{ margin: 8 }} onClick={() => setSearchOpen(true)}>
        + Add coin
      </button>
      {searchOpen && <SearchDialog onClose={() => setSearchOpen(false)} />}
      {alertSymbol && <AlertEditor symbol={alertSymbol} onClose={() => setAlertSymbol(null)} />}
    </div>
  );
}
