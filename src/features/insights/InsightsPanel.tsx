import { useEffect, useState } from "react";
import { baseAsset } from "../../core/format/symbol";
import { useInsightsStore } from "../../core/store/insights";
import { useSettingsStore } from "../../core/store/settings";
import { useWatchlistStore } from "../../core/store/watchlist";
import { CoinReport } from "./CoinReport";
import { InsightsSettingsForm } from "./InsightsSettingsForm";
import { MarketScanReport } from "./MarketScanReport";

type Mode = "coin" | "scan";

/**
 * The AI tab: research one coin, or scan the market for projects.
 *
 * Every other tab in this app shows something that is already true. This one shows an argument,
 * and it costs money to produce — so the panel is built around three rules:
 *
 * - **No call without a click.** Opening the tab, switching symbols and reopening the panel all
 *   read the disk cache. Only "Research" and "Refresh" reach the model.
 * - **The age of an answer is never hidden.** A cached report is labelled and dated in its
 *   footer; refreshing it is one button away.
 * - **Measured and argued are visually separate.** CoinGecko's figures and the search citations
 *   sit in their own blocks, so the model's prose is never mistaken for data.
 */
export function InsightsPanel() {
  const loaded = useInsightsStore((s) => s.loaded);
  const load = useInsightsStore((s) => s.load);
  const state = useInsightsStore((s) => s.state);
  const saveSettings = useInsightsStore((s) => s.saveSettings);
  const symbol = useInsightsStore((s) => s.symbol);
  const coin = useInsightsStore((s) => s.coin);
  const scan = useInsightsStore((s) => s.scan);
  const busy = useInsightsStore((s) => s.busy);
  const error = useInsightsStore((s) => s.error);
  const dismissError = useInsightsStore((s) => s.dismissError);
  const showCoin = useInsightsStore((s) => s.showCoin);
  const researchCoin = useInsightsStore((s) => s.researchCoin);
  const researchMarket = useInsightsStore((s) => s.researchMarket);

  const watchlist = useWatchlistStore((s) => s.items);
  const pinnedSymbol = useSettingsStore((s) => s.settings?.pinnedSymbol ?? null);

  const [mode, setMode] = useState<Mode>("coin");
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  // Land on whatever the user is already looking at — the pinned symbol, else the first row —
  // and pull that coin's cached report in. Free: `showCoin` never calls the model.
  useEffect(() => {
    if (symbol || watchlist.length === 0) return;
    void showCoin(pinnedSymbol ?? watchlist[0].symbol);
  }, [symbol, watchlist, pinnedSymbol, showCoin]);

  if (!loaded || !state) {
    return (
      <div className="panel__body">
        <div className="onboarding">
          <span>Loading…</span>
        </div>
      </div>
    );
  }

  if (!state.key.present) {
    return (
      <div className="panel__body insights">
        <div className="insights__intro">
          <strong>AI research</strong>
          <span>
            Ask Claude what is happening to a coin, with live web search and a link behind every
            claim. Bring your own Anthropic key — the widget never proxies the call.
          </span>
        </div>
        <InsightsSettingsForm />
      </div>
    );
  }

  if (!state.settings.enabled) {
    return (
      <div className="panel__body insights">
        <div className="onboarding">
          <span>AI research is switched off.</span>
          <button className="onboarding__cta" onClick={() => void saveSettings({ enabled: true })}>
            Turn it on
          </button>
        </div>
        <InsightsSettingsForm />
      </div>
    );
  }

  const activeSymbol = symbol ?? pinnedSymbol ?? watchlist[0]?.symbol ?? null;
  const asset = activeSymbol ? baseAsset(activeSymbol) : "";

  return (
    <div className="panel__body insights">
      <div className="insights__toolbar">
        <div className="insights__modes">
          <button
            className={`panel-tab ${mode === "coin" ? "panel-tab--active" : ""}`}
            onClick={() => setMode("coin")}
          >
            COIN
          </button>
          <button
            className={`panel-tab ${mode === "scan" ? "panel-tab--active" : ""}`}
            onClick={() => setMode("scan")}
          >
            SCAN
          </button>
        </div>
        <button
          className={`icon-btn ${showSettings ? "icon-btn--active" : ""}`}
          title="AI settings"
          onClick={() => setShowSettings((on) => !on)}
        >
          ⚙
        </button>
      </div>

      {showSettings && <InsightsSettingsForm onDone={() => setShowSettings(false)} />}

      {error && (
        <div className="insights__error" onClick={dismissError} title="Dismiss">
          {error}
        </div>
      )}

      {mode === "coin" && (
        <>
          <div className="insights__row">
            <select
              className="select-input"
              value={activeSymbol ?? ""}
              disabled={watchlist.length === 0 || busy}
              onChange={(e) => void showCoin(e.target.value)}
            >
              {watchlist.map((item) => (
                <option key={item.symbol} value={item.symbol}>
                  {item.symbol}
                </option>
              ))}
            </select>
            <button
              className="btn btn--primary"
              disabled={!activeSymbol || busy}
              onClick={() => activeSymbol && void researchCoin(activeSymbol, !!coin)}
            >
              {busy ? "Researching…" : coin ? "Refresh" : `Research ${asset}`}
            </button>
          </div>

          {watchlist.length === 0 && (
            <div className="settings-panel__hint">
              Add a coin to the watchlist first — this tab researches what you follow.
            </div>
          )}

          {busy && <div className="insights__working">Searching the web and writing the report…</div>}
          {!busy && coin && <CoinReport insight={coin} />}
          {!busy && !coin && watchlist.length > 0 && (
            <div className="settings-panel__hint">
              No report for {asset} yet. One call costs tokens plus a charge per web search; the
              answer is then reused until it expires.
            </div>
          )}
        </>
      )}

      {mode === "scan" && (
        <>
          <div className="insights__row">
            <span className="settings-panel__label">Projects worth a look</span>
            <button
              className="btn btn--primary"
              disabled={busy}
              onClick={() => void researchMarket(!!scan)}
            >
              {busy ? "Scanning…" : scan ? "Refresh" : "Scan market"}
            </button>
          </div>

          {busy && <div className="insights__working">Searching the web and writing the report…</div>}
          {!busy && scan && <MarketScanReport scan={scan} />}
          {!busy && !scan && (
            <div className="settings-panel__hint">
              A scan reads this week's news and comes back with a handful of projects, each with a
              catalyst you can check and the risk that breaks it.
            </div>
          )}
        </>
      )}

      <div className="insights__disclaimer">
        Opinions produced by a language model from web search. Verify before acting — this is not
        financial advice.
      </div>
    </div>
  );
}
