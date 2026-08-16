import { useEffect } from "react";
import { commands } from "../core/ipc/commands";
import {
  onAlerts,
  onConnection,
  onExpanded,
  onOpenSettings,
  onPinned,
  onTickers,
} from "../core/ipc/events";
import { useAlertsStore } from "../core/store/alerts";
import { useSettingsStore } from "../core/store/settings";
import { useTickersStore } from "../core/store/tickers";
import { useUiStore } from "../core/store/ui";
import { useWatchlistStore } from "../core/store/watchlist";
import { Pill } from "../features/pill/Pill";
import { usePillDrag } from "../features/pill/usePillDrag";
import { SettingsPanel } from "../features/settings/SettingsPanel";
import { StatusBar } from "../features/settings/StatusBar";
import { WatchlistPanel } from "../features/watchlist/WatchlistPanel";

const FX_REFRESH_MS = 6 * 60 * 60 * 1000;

export function App() {
  const expanded = useUiStore((s) => s.expanded);
  const setExpanded = useUiStore((s) => s.setExpanded);
  const settingsOpen = useUiStore((s) => s.settingsOpen);
  const setSettingsOpen = useUiStore((s) => s.setSettingsOpen);
  const setConnection = useUiStore((s) => s.setConnection);
  const setFxRate = useUiStore((s) => s.setFxRate);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const setWindow = useSettingsStore((s) => s.setWindow);
  const setWatchlistItems = useWatchlistStore((s) => s.setItems);
  const setAlertItems = useAlertsStore((s) => s.setItems);
  const applyTickersSnapshot = useTickersStore((s) => s.applySnapshot);
  const { onPointerDown } = usePillDrag();

  useEffect(() => {
    void (async () => {
      const [loadedSettings, tickers, connection, isExpanded] = await Promise.all([
        commands.getSettings(),
        commands.getTickers(),
        commands.getConnection(),
        commands.getExpanded(),
      ]);
      setExpanded(isExpanded);
      setSettings(loadedSettings);
      setWatchlistItems(loadedSettings.watchlist);
      setAlertItems(loadedSettings.alerts);
      applyTickersSnapshot(tickers);
      setConnection(connection);
      if (loadedSettings.display.fiat) {
        setFxRate(await commands.getFxRate());
      }
    })();

    const unlistenPromises = [
      onTickers(applyTickersSnapshot),
      onConnection(setConnection),
      onOpenSettings(() => setSettingsOpen(true)),
      onExpanded((next) => {
        setExpanded(next);
        // A collapsed pill has nowhere to render the settings pane; leaving the flag set would
        // make it reappear unbidden on the next expand.
        if (!next) setSettingsOpen(false);
      }),
      onPinned((pinned) => {
        const current = useSettingsStore.getState().settings;
        if (current) useSettingsStore.getState().setWindow({ ...current.window, pinned });
      }),
      onAlerts(setAlertItems),
    ];

    return () => {
      void Promise.all(unlistenPromises).then((fns) => fns.forEach((fn) => fn()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!settings?.display.fiat) return;
    const id = setInterval(() => {
      void commands.getFxRate().then(setFxRate);
    }, FX_REFRESH_MS);
    return () => clearInterval(id);
  }, [settings?.display.fiat, setFxRate]);

  if (!expanded) {
    return (
      <div className="app-root">
        <Pill />
      </div>
    );
  }

  const handleTogglePin = () => {
    if (!settings) return;
    const pinned = !settings.window.pinned;
    void commands.setPin(pinned).then(() => setWindow({ ...settings.window, pinned }));
  };

  return (
    <div className="app-root">
      <div className="panel">
        <div className="panel__header" onPointerDown={onPointerDown}>
          <span className="panel__title">CRYPTO-WIDGET</span>
          <div className="panel__actions">
            <button
              className={`icon-btn ${settings?.window.pinned ? "icon-btn--active" : ""}`}
              title={settings?.window.pinned ? "Unpin (auto-collapse on blur)" : "Pin (stay open)"}
              onClick={handleTogglePin}
            >
              📌
            </button>
            <button className="icon-btn" title="Settings" onClick={() => setSettingsOpen(true)}>
              ⚙
            </button>
            <button
              className="icon-btn"
              title="Collapse"
              onClick={() => void commands.toggleExpand().then(setExpanded)}
            >
              ▸
            </button>
          </div>
        </div>
        <div style={{ position: "relative", flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
          <WatchlistPanel />
          {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
        </div>
        <StatusBar />
      </div>
    </div>
  );
}
