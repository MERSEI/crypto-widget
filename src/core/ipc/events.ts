import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FuturesState } from "../../types/futures";
import type { ConnectionStatus, TickerSnapshot } from "../../types/market";
import type { Alert } from "../../types/settings";

export function onTickers(handler: (snapshot: TickerSnapshot[]) => void): Promise<UnlistenFn> {
  return listen<TickerSnapshot[]>("tickers", (event) => handler(event.payload));
}

export function onConnection(handler: (status: ConnectionStatus) => void): Promise<UnlistenFn> {
  return listen<ConnectionStatus>("connection", (event) => handler(event.payload));
}

export function onOpenSettings(handler: () => void): Promise<UnlistenFn> {
  return listen<void>("open-settings", () => handler());
}

// Rust owns the expanded/pinned flags — the tray and blur-autocollapse change them without the
// renderer ever invoking a command, so these events are what keep the UI in sync.
export function onExpanded(handler: (expanded: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("expanded", (event) => handler(event.payload));
}

export function onPinned(handler: (pinned: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("pinned", (event) => handler(event.payload));
}

// The futures account is polled by Rust on its own schedule, whether or not the tab is open —
// so the panel is a listener, not a poller.
export function onFutures(handler: (state: FuturesState) => void): Promise<UnlistenFn> {
  return listen<FuturesState>("futures", (event) => handler(event.payload));
}

// Emitted when an alert fires: the rule itself changes (`lastFiredAt`, and `enabled` for a
// one-shot), so the panel has to reload the list rather than trust its own copy.
export function onAlerts(handler: (alerts: Alert[]) => void): Promise<UnlistenFn> {
  return listen<Alert[]>("alerts", (event) => handler(event.payload));
}
