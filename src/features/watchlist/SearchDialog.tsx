import { useEffect, useRef, useState } from "react";
import { commands } from "../../core/ipc/commands";
import { formatVolume } from "../../core/format/volume";
import { useWatchlistStore } from "../../core/store/watchlist";
import type { PairInfo } from "../../types/market";

interface Props {
  onClose: () => void;
}

export function SearchDialog({ onClose }: Props) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<PairInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const inputRef = useRef<HTMLInputElement>(null);
  const add = useWatchlistStore((s) => s.add);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    const handle = setTimeout(() => {
      commands
        .searchPairs(query)
        .then((pairs) => {
          if (!cancelled) setResults(pairs);
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, 150);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [query]);

  const handlePick = (symbol: string) => {
    void add(symbol);
    onClose();
  };

  return (
    <div className="search-dialog">
      <div className="search-dialog__header">
        <input
          ref={inputRef}
          className="search-dialog__input"
          placeholder="Search pair…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onClose()}
        />
        <button className="icon-btn" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="search-dialog__results">
        {!loading && results.length === 0 && (
          <div className="search-dialog__empty">No pairs found</div>
        )}
        {results.map((pair) => (
          <div key={pair.symbol} className="search-result-row" onClick={() => handlePick(pair.symbol)}>
            <span>{pair.symbol}</span>
            <span className="search-result-row__volume mono-nums">{formatVolume(pair.quoteVolume)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
