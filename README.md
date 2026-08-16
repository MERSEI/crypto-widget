# crypto-widget

A resident desktop widget for peripheral crypto-market awareness — a draggable pill docked to
a screen edge that expands into a compact terminal-style panel: a personal watchlist with live
Binance prices, per-coin charts, and price/spike alerts delivered as Windows toasts.

Not a trading terminal, not a portfolio tracker. Built to answer "where's the market right now"
in half a second without switching windows.

## Features

- **Docked pill.** A 140×30 bar snapped to any screen edge; drag it anywhere and it re-docks to
  the nearest edge. Position is stored as a resolution-independent fraction, so it survives
  restarts, DPI changes, and monitor swaps.
- **Live watchlist.** Prices, 24h change, and quote volume streamed from Binance's public
  combined WebSocket, coalesced to at most 4 UI updates per second. Drag-and-drop reordering
  (`@dnd-kit`), fuzzy pair search over the cached `exchangeInfo` catalog.
- **Charts.** Per-coin accordion with `lightweight-charts` — area or candlestick, 1h/4h/1d/1w
  timeframes, klines pulled on demand from the REST API.
- **Alerts.** `price_above`, `price_below`, and rolling-window `spike` rules with cooldown and
  one-shot modes. Evaluated in Rust, so they keep firing while the panel is collapsed; delivery
  is a native Windows toast.
- **Resilient by design.** Exponential backoff with jitter on WS failure, automatic REST polling
  fallback after 3 failed attempts, stale-tick detection, and a warmup window after reconnect so
  a connection gap can never be mistaken for a price spike. The last valid numbers stay on
  screen — the panel never blanks out.
- **Optional fiat column.** USD→CZK conversion from a cached FX endpoint with a primary/fallback
  pair and a 6h TTL.
- **Tray integration.** Show/hide, pin, settings, and quit from the system tray.

No API keys, no accounts, no telemetry — every endpoint the app touches is public.

## Stack

Tauri 2 + React 19 + TypeScript + Vite on the frontend; the WebSocket client, alert evaluation,
window geometry, and settings persistence live in Rust (`src-tauri/`) so alerts keep firing while
the panel is collapsed. State on the JS side is Zustand; there is no HTTP client in the renderer —
all data crosses the Tauri IPC boundary.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the data flow, the connection state
machine, the IPC contract, and the settings schema.

## Requirements

- Node.js + npm
- Rust (`rustup`, `stable-x86_64-pc-windows-msvc`)
- Windows: MSVC Build Tools 2022 + WebView2 Runtime (both usually already present)

The app is developed and tested on Windows. The Tauri/React code is not Windows-specific, but
toast delivery, tray behaviour, and work-area detection have only been exercised there.

## Development

```bash
npm install
npm run tauri dev
```

Vite serves the renderer on `http://localhost:1420`; Tauri opens the pill window against it.

## Checks

```bash
npx tsc --noEmit
cd src-tauri && cargo check
```

## Tests

```bash
npm run test                 # vitest — format helpers, ticker store selectors
cd src-tauri && cargo test   # spike detector, alert engine, backoff, REST/WS parsers, config I/O
```

## Build

```bash
npm run tauri build
```

Produces a portable `.exe` plus NSIS/MSI installers under
`src-tauri/target/release/bundle/`. The installer is **not code-signed** — Windows SmartScreen
will warn on first run ("Windows protected your PC" → More info → Run anyway). This is expected
for the MVP; code signing is out of scope.

## Configuration

There is nothing to configure before first run. `.env.example` documents optional endpoint
overrides (Binance REST/WS base URLs, FX host) that are only useful for pointing the app at a
mirror during testing.

Runtime state lives in the Tauri app config directory
(`%APPDATA%\com.flowe.crypto-widget` on Windows):

| Path             | Contents                                                          |
| ---------------- | ----------------------------------------------------------------- |
| `settings.json`  | Window position, display/chart preferences, watchlist, alert rules |
| `cache/`         | TTL-cached `exchangeInfo` catalog, FX rate, last known tickers     |

`settings.json` is versioned (`version: 1`) and written at most twice a second by a background
flush task. A corrupt file is backed up rather than deleted, and the app falls back to defaults.

## Project layout

```
src/                      React renderer
  app/                    App shell, error boundary, theme
  core/format/            Price / percent / volume formatting
  core/ipc/               Typed wrappers over Tauri invoke + event listeners
  core/store/             Zustand stores (tickers, watchlist, alerts, settings, ui)
  features/               Pill, watchlist, chart, alerts, settings panels
  types/                  Shared market + settings types (mirror the Rust structs)
src-tauri/src/            Rust backend
  market/                 Binance WS client, REST provider, FX provider, market hub
  alerts/                 Spike detector + alert engine
  commands.rs             Every #[tauri::command] exposed to the renderer
  config.rs               Settings schema, load/save, TTL cache helpers
  window.rs               Edge docking, geometry, pill ↔ panel transitions
  tray.rs                 System tray menu
tests/                    Vitest suites
docs/                     Architecture notes
```

## Manual checklist

- Drag the pill to each screen edge, restart the app — position must survive.
- DPI 100% / 125% / 150% — pill still docks correctly.
- Add 20+ coins to the watchlist — ticks and scroll stay smooth.
- Restart the app with an accordion open — no crash, chart reloads on next open.
- Disconnect the network — status goes `reconnecting` then `polling`, last valid numbers stay
  on screen (never blank). Reconnect — status returns to `live` within ~30s.
- Change display resolution while running — geometry recalculates on next drag/toggle.
- Leave it running for 24h — RAM stays near idle target, no leak-driven growth.

## Known MVP limitations

- **Alerts only fire while the app is running.** Autostart is wired via
  `tauri-plugin-autostart` but left disabled by default (user decision, not a technical gap) —
  enable it in Settings if you want alerts to survive a reboot without a manual relaunch.
- **Work-area detection uses the monitor's full bounds**, not the OS-reported work area minus
  the taskbar (that needs Win32 GDI FFI). In practice this only matters for bottom-edge
  docking on a machine with a bottom taskbar; right/left/top docking are unaffected.
- No portfolio, PnL, exchange API keys, DeFi wallets, futures, or indicators — out of scope by
  design, see the original spec's "not-goals" section.

## License

MIT — see [LICENSE](LICENSE).
