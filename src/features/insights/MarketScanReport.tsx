import { useState } from "react";
import { formatAge } from "../../core/format/age";
import { useInsightsStore } from "../../core/store/insights";
import { useWatchlistStore } from "../../core/store/watchlist";
import type { MarketScan, ProjectIdea } from "../../types/insights";
import { hostOf } from "./CoinReport";

/** One idea. Collapsed to its thesis until opened — six expanded cards do not fit a 380px
 *  panel, and the thesis is what decides whether the rest is worth reading. */
function IdeaCard({ idea }: { idea: ProjectIdea }) {
  const [open, setOpen] = useState(false);
  const openUrl = useInsightsStore((s) => s.openUrl);
  const watchlist = useWatchlistStore((s) => s.items);
  const addToWatchlist = useWatchlistStore((s) => s.add);

  const alreadyWatched =
    !!idea.binanceSymbol && watchlist.some((item) => item.symbol === idea.binanceSymbol);

  return (
    <div className={`insights-idea ${open ? "insights-idea--open" : ""}`}>
      <button className="insights-idea__head" onClick={() => setOpen((on) => !on)}>
        <span className="insights-idea__name">
          {idea.name}
          {idea.symbol && <span className="insights-idea__ticker mono-nums">{idea.symbol}</span>}
        </span>
        <span className="insights-idea__conviction mono-nums" title="The model's own conviction">
          {idea.conviction}
        </span>
      </button>

      {idea.category && <div className="insights-idea__category">{idea.category}</div>}
      {idea.thesis && <p className="insights-idea__thesis">{idea.thesis}</p>}

      {open && (
        <div className="insights-idea__detail">
          {idea.catalyst && (
            <div>
              <span className="insights-idea__label">Catalyst</span> {idea.catalyst}
            </div>
          )}
          {idea.risk && (
            <div>
              <span className="insights-idea__label">Risk</span> {idea.risk}
            </div>
          )}
          {idea.horizon && (
            <div>
              <span className="insights-idea__label">Horizon</span> {idea.horizon}
            </div>
          )}
          <div className="insights-idea__actions">
            {idea.url && (
              <button className="btn" onClick={() => void openUrl(idea.url as string)}>
                {hostOf(idea.url)} ↗
              </button>
            )}
            {/* No button at all when Binance has no pair for it: the alternative is a watchlist
                row that never shows a price. */}
            {idea.binanceSymbol && (
              <button
                className="btn"
                disabled={alreadyWatched}
                onClick={() => void addToWatchlist(idea.binanceSymbol as string)}
              >
                {alreadyWatched ? "In watchlist" : `+ ${idea.binanceSymbol}`}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export function MarketScanReport({ scan }: { scan: MarketScan }) {
  const openUrl = useInsightsStore((s) => s.openUrl);
  const addToWatchlist = useWatchlistStore((s) => s.add);
  const watchlist = useWatchlistStore((s) => s.items);

  return (
    <div className="insights-card">
      {scan.narrative && <p className="insights-card__summary">{scan.narrative}</p>}

      {scan.ideas.length > 0 && (
        <div className="insights-card__block">
          <div className="insights-card__block-title">Ideas</div>
          {scan.ideas.map((idea, index) => (
            <IdeaCard key={`${idea.symbol}-${index}`} idea={idea} />
          ))}
        </div>
      )}

      {scan.trending.length > 0 && (
        <div className="insights-card__block">
          <div className="insights-card__block-title">Trending on CoinGecko — measured</div>
          <div className="insights-card__tags">
            {scan.trending.slice(0, 10).map((coin) => {
              const watched =
                !!coin.binanceSymbol && watchlist.some((item) => item.symbol === coin.binanceSymbol);
              return (
                <button
                  className={`insights-tag ${coin.binanceSymbol ? "insights-tag--link" : ""}`}
                  key={coin.id}
                  disabled={!coin.binanceSymbol || watched}
                  title={
                    coin.binanceSymbol
                      ? `Add ${coin.binanceSymbol} to the watchlist`
                      : "No Binance USDT pair"
                  }
                  onClick={() => coin.binanceSymbol && void addToWatchlist(coin.binanceSymbol)}
                >
                  {coin.symbol}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {scan.sources.length > 0 && (
        <div className="insights-card__block">
          <div className="insights-card__block-title">Sources read ({scan.sources.length})</div>
          <div className="insights-card__tags">
            {scan.sources.map((source) => (
              <button
                className="insights-tag insights-tag--link"
                key={source.url}
                title={source.url}
                onClick={() => void openUrl(source.url)}
              >
                {hostOf(source.url)}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="insights-card__footer">
        <span>{scan.model}</span>
        <span className="mono-nums">
          {scan.usage.webSearches} searches ·{" "}
          {(scan.usage.inputTokens + scan.usage.outputTokens).toLocaleString()} tok
        </span>
        <span>
          {formatAge(scan.generatedAt)}
          {scan.cached && " · cached"}
        </span>
      </div>
    </div>
  );
}
