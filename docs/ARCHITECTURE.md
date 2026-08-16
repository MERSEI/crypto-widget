# Architecture

How crypto-widget is put together, and why. Read [`../README.md`](../README.md) first for what
the app does.

## The one design rule

**Rust owns the state; React renders it.**

Every piece of state that must survive a collapsed panel — live prices, the connection status,
alert rules and their cooldowns, the window's docked position, expanded/pinned flags — lives in
the Rust process. The renderer holds a mirror of it in Zustand, hydrated once on mount and kept
in sync by events.

This is not a stylistic preference. The panel spends most of its life collapsed to a 140×30 pill,
and the webview is free to do whatever it wants in that state. If alert evaluation lived in
React, a collapsed widget would be a widget that silently stops working. So the renderer never
opens a socket, never calls Binance, and never owns a number anyone depends on.

## Process layout

```
┌─ Rust (src-tauri) ──────────────────────────────────────────────┐
│                                                                 │
│  binance_ws ──ticks──▶ MarketHub ──▶ AlertsEngine ──▶ toast      │
│      │                    │  │                                  │
│      │ fallback           │  └─ PriceBuffers (rolling window)    │
│      ▼                    │                                     │
│  binance_rest ────────────┘                                     │
│      ▲                    │                                     │
│      │ klines/search      │ emit @ ≤4 Hz                        │
│  ConfigState ◀── autosave ┘                                     │
│  (settings.json)          │                                     │
└───────────────────────────┼─────────────────────────────────────┘
                            │ Tauri IPC (commands + events)
┌─ React (src) ─────────────▼─────────────────────────────────────┐
│  core/ipc ──▶ Zustand stores ──▶ features/{pill,watchlist,…}     │
└─────────────────────────────────────────────────────────────────┘
```

`AppState` (in `lib.rs`) is the single managed struct every command reads from: `ConfigState`,
`MarketHub`, the WS handle, the market provider, the FX provider, plus the `expanded` flag and a
`move_gen` counter used to debounce edge snapping.

## Market data flow

1. **`binance_ws`** holds one combined-stream socket (`wss://stream.binance.com:9443/stream`)
   subscribed to `<symbol>@ticker` for every watchlist entry. Changing the watchlist calls
   `WsHandle::set_symbols`, which resubscribes rather than reconnecting.
2. Each frame is parsed into a `TickerSnapshot { symbol, price, percent24h, quoteVolume, eventTime }`
   and handed to `MarketHub::apply_ticker`.
3. **`MarketHub`** is the in-memory truth: a `HashMap<symbol, TickerSnapshot>`, the connection
   status, and the rolling price buffers. `apply_ticker` pushes into the buffer, runs the alert
   engine, stores the snapshot, and sets a `dirty` flag.
4. A background loop emits the whole ticker map on the `tickers` event **at most 4× per second**,
   and only when `dirty` — bursty WS traffic coalesces into one React render instead of dozens.
5. On startup the watchlist is seeded from REST before the first WS tick arrives, so the panel
   shows real numbers on frame one instead of `···` — which for a halted pair would otherwise be
   forever.

### Connection state machine

`ConnectionState` is one of `connecting → live → stale → reconnecting → polling → offline`, and it
is surfaced verbatim in the status bar.

| Transition                 | Trigger                                                       |
| -------------------------- | ------------------------------------------------------------- |
| → `live`                   | Socket open, frames arriving                                   |
| → `stale`                  | No frame for `STALE_TIMEOUT` (10s)                             |
| → `reconnecting`           | Socket closed or errored; backoff `1→2→4→8→16→30s` ±jitter     |
| → `polling`                | 3 consecutive failed connect attempts → REST poll every 10s    |
| `polling` → `connecting`   | Retried socket every 60s while polling                         |
| planned reconnect          | Every 23h, ahead of Binance's own 24h server-side disconnect   |

Two invariants matter here:

- **The UI never blanks.** A failed connection changes the status badge; the last valid prices
  stay on screen.
- **A reconnect can't fake a spike.** `MarketHub::on_connected` clears the rolling buffers and
  opens a warmup window equal to the longest enabled spike window. Until it expires, spike rules
  are skipped — otherwise the price gap across a 5-minute outage would read as a 5-minute move.

### REST usage

`BinanceProvider` fronts every REST call with a **token bucket** (weight/sec), because a runaway
loop against `api.binance.com` earns an IP ban, not an error. Weights: catalog 80, klines 2,
ticker poll 2×symbols.

Responses are cached on disk with a TTL, and a *stale* cache read is preferred over failure:

| Call                    | Endpoint                          | Cache                        |
| ----------------------- | --------------------------------- | ---------------------------- |
| Pair catalog / search   | `/api/v3/ticker/24hr`             | `exchange_info.json`, 24h TTL |
| Chart candles           | `/api/v3/klines` (limit ≤200)     | none — on demand              |
| Polling fallback        | `/api/v3/ticker/24hr?symbols=[…]` | none                          |
| USD→CZK rate            | FX host, primary + fallback pair  | 6h TTL                        |

Search filters the cached catalog (USDT pairs only) by symbol/base substring, sorts by quote
volume, and truncates to 50 — no network round-trip per keystroke.

## Alerts

Three rule kinds share one evaluation path, run on every incoming tick for the matching symbol:

- `price_above` — `price >= value`
- `price_below` — `price <= value`
- `spike` — `|change| >= value%` over `windowMinutes`, comparing the latest price to the oldest
  point still inside the window (`alerts/spike.rs`)

Firing is gated by `cooldownMin` (per rule, based on `lastFiredAt`) and by `once`, which disables
the rule after it fires. Because firing *mutates the rule*, the backend re-emits the whole alert
list on the `alerts` event — the panel can't trust its own copy.

`evaluate_pure` is deliberately separated from the `AppHandle`: the decision logic is a pure
function over `(alerts, symbol, price, buffer, warmup, now)` and is unit-tested without a running
Tauri app. Notification delivery is the impure half.

`PriceBuffers` retention is the longest enabled spike window + 60s. That bound is load-bearing:
Binance pushes ~1 tick/sec/symbol, so a naive 24h retention parks ~86k points per coin in RAM to
serve a detector that never looks past its own window.

## Window behaviour

`window.rs` owns geometry; nothing about position lives in CSS or React.

- The pill is **140×30 horizontal**, not a vertical sliver — rotated text in a 28px strip was
  unreadable at a glance, which defeats the entire point of the widget.
- Position is stored as `{ edge, offset }` where `offset` is a **fraction** of the edge's length,
  making it resolution- and DPI-independent. `edge_offset_from_position` recomputes it after every
  manual drag.
- Edge snapping is driven by `WindowEvent::Moved` and debounced through the `move_gen` counter;
  the apply step is a no-op when the window is already docked, because repositioning emits another
  `Moved` and an unconditional apply would loop forever.
- Expanding swaps pill geometry for panel geometry (`panelWidth × panelHeight`, default 380×520)
  against the same edge anchor.
- Blur auto-collapse re-checks focus after 220ms before acting: WebView2 briefly drops window
  focus whenever it spins up a native surface (opening a chart is the reliable trigger), which
  used to collapse the panel out from under a mid-click user. `pinned` disables auto-collapse
  entirely; always-on-top is unconditional.
- Work area is approximated by the monitor's full bounds — see the README's limitations.

## Persistence

`ConfigState` loads `settings.json` from the Tauri app config dir on startup, keeps it behind an
`RwLock`, and a background task flushes it **at most twice a second** when dirty. Commands mutate
the struct and call `mark_dirty()` — no command writes to disk synchronously.

- Schema is versioned (`SETTINGS_VERSION = 1`); the Rust structs are `camelCase`-serialized and
  mirrored by `src/types/settings.ts`. **Changing one means changing the other.**
- A corrupt file is *backed up*, not deleted, and the app starts from defaults.
- `cache/` holds TTL envelopes (`saved_at` + value) for the catalog, FX rate, and last tickers.

## IPC contract

All commands are declared in `src-tauri/src/commands.rs`, registered in `lib.rs`, and wrapped
type-safely in `src/core/ipc/commands.ts`. Reads first, then mutations, then window control:

| Command                                  | Purpose                                        |
| ---------------------------------------- | ---------------------------------------------- |
| `get_settings` / `get_tickers` / `get_connection` / `get_expanded` | Initial hydration     |
| `search_pairs` / `get_klines` / `get_fx_rate`                      | On-demand market data |
| `add_watchlist_symbol` / `remove_watchlist_symbol` / `reorder_watchlist` | Watchlist       |
| `set_display` / `set_chart_settings` / `set_notifications` / `set_autostart` | Settings   |
| `upsert_alert` / `delete_alert`          | Alert rules                                    |
| `start_drag` / `drag_ended` / `set_pin` / `toggle_expand` / `collapse_if_unpinned` | Window |

Events flow the other way (`src/core/ipc/events.ts`):

| Event           | Payload             | Emitted when                                     |
| --------------- | ------------------- | ------------------------------------------------ |
| `tickers`       | `TickerSnapshot[]`  | ≤4×/sec while prices change                      |
| `connection`    | `ConnectionStatus`  | Connection state or attempt count changes        |
| `alerts`        | `Alert[]`           | A rule fires and mutates itself                  |
| `expanded`      | `boolean`           | Tray toggle or blur auto-collapse                |
| `pinned`        | `boolean`           | Tray pin toggle                                  |
| `open-settings` | —                   | Tray "Settings"                                  |

`expanded`/`pinned` exist as events precisely because the tray and the auto-collapse path change
them without the renderer ever invoking a command.

## Frontend structure

Zustand, one store per concern, no global god-object:

- `tickers` — the price map, replaced wholesale on each `tickers` event
- `watchlist` — ordered symbols, optimistic on drag, authoritative from settings
- `alerts` — rule list, refreshed from the `alerts` event
- `settings` — the mirrored `AppSettings`
- `ui` — expanded/settings-open flags, connection status, FX rate

`features/` holds the visual units (pill + drag hook, watchlist rows and search dialog, chart
accordion with timeframe bar, alert editor and list, settings panel and status bar). `core/format/`
keeps price/percent/volume formatting pure and tested — significant-digit handling for sub-cent
prices is the kind of thing that silently regresses.

## Testing

| Layer          | Command             | Covers                                                                   |
| -------------- | ------------------- | ------------------------------------------------------------------------ |
| TypeScript     | `npm run test`      | Format helpers, ticker store selectors                                    |
| Rust           | `cargo test`        | Spike detector, alert engine decision logic, WS backoff, kline/ticker parsing (against captured real payloads), settings load/corrupt-recovery |

The parser tests deliberately feed real API payloads including malformed and extra-column rows:
Binance adds trailing kline columns over time, and `"NaN".parse::<f64>()` succeeds — both were
live panic paths before they were tests.
