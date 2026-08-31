# crypto-widget

[![CI](https://github.com/MERSEI/crypto-widget/actions/workflows/ci.yml/badge.svg)](https://github.com/MERSEI/crypto-widget/actions/workflows/ci.yml)

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
  one-shot modes. Level rules are edge-triggered: they fire on a crossing, not on a price that
  already sits past the level, and re-arm once it comes back. A stale price never fires anything. Evaluated in Rust, so they keep firing while the panel is collapsed; delivery
  is a native Windows toast.
- **Backup venues.** When Binance stops quoting a watchlist pair — a halted pair keeps serving
  the price of its last trade forever — OKX, Bybit, KuCoin, and Gate are asked in turn, and the
  row shows which venue answered. A price no one can refresh is marked `STALE` instead of
  passing for live.
- **Resilient by design.** Exponential backoff with jitter on WS failure, automatic REST polling
  fallback after 3 failed attempts, stale-tick detection, and a warmup window after reconnect so
  a connection gap can never be mistaken for a price spike. The last valid numbers stay on
  screen — the panel never blanks out.
- **Optional fiat column.** USDT→CZK conversion from a cached FX endpoint with a primary/fallback
  pair and a 6h TTL.
- **Futures positions.** Optional, off by default: a read-only mirror of an exchange futures
  account — wallet and margin balance, open positions, unrealised PnL and ROE, all computed in
  Rust so the panel and the backend cannot disagree. Orders are only ever sent to the testnet.
- **Wallet.** An Ethereum wallet in its own window: balances for ETH and any ERC-20 you add,
  transaction history, and transfers behind a review-then-confirm step. Keys are derived from a
  BIP-39 phrase at the standard BIP-44 path and stored in the Windows Credential Manager — the
  phrase never reaches the interface except when you ask to see it for a backup, and signing
  happens entirely in Rust. Token decimals are read from the contract, amounts never touch a JS
  `number`, and a fee that has run away from the one you approved is refused rather than signed.
- **AI research.** Optional, off by default: an "AI" tab that asks Claude what is happening to a
  coin you follow, or scans the market for projects worth a look. The model runs live web
  searches, and every headline and citation is a link you can open — the panel drops any it
  cannot link. CoinGecko's figures (rank, market cap, supply, distance from the all-time high)
  are fetched separately and shown in their own block, so the measured half is never mistaken
  for the argued one. Answers are cached on disk for a window you choose; only the refresh
  button pays again, and each card shows the model, the search count, the tokens, and the age of
  the answer. Bring your own Anthropic API key — it is stored in the Windows Credential Manager
  and the call goes straight from your machine to the API.
- **Referral links.** Partner affiliate links built from your own IDs, with a QR code.
- **Tray integration.** Show/hide, pin, wallet, settings, and quit from the system tray.

No telemetry, and no account is needed for anything the widget shows by default — the market data
endpoints are all public. The optional features (futures, wallet, Etherscan history, AI research)
take credentials, and those live in the Windows Credential Manager, never in `settings.json` and
never in the renderer.

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

Both suites plus the typecheck, the renderer build, and `clippy -D warnings` run in CI
([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) on every push and pull request — the
frontend job on Linux, the Rust job on Windows, since that is the platform the tray, toasts,
and window geometry actually target.

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
| `cache/`         | TTL-cached `exchangeInfo` catalog, FX rate, last known tickers, AI reports |

`settings.json` is versioned (`version: 4`) and written at most twice a second by a background
flush task. A corrupt file is backed up rather than deleted, and the app falls back to defaults.

## Project layout

```
src/                      React renderer
  app/                    App shell, error boundary, theme
  core/format/            Price / percent / volume / age / symbol formatting
  core/ipc/               Typed wrappers over Tauri invoke + event listeners
  core/store/             Zustand stores (tickers, watchlist, alerts, settings, ui, wallet, insights)
  features/               Pill, watchlist, chart, alerts, settings, futures, referral, wallet, insights
  types/                  Shared market + settings + wallet + insights types (mirror the Rust structs)
src-tauri/src/            Rust backend
  market/                 Binance WS client, REST provider, FX provider, market hub
  alerts/                 Spike detector + alert engine
  futures/                Signed exchange client, account hub, futures commands
  referral/               Partner catalogue and link building
  wallet/                 Keystore (BIP-39/44), chain client, Etherscan history, commands
  insights/               Anthropic client, CoinGecko fundamentals, research commands
  secrets.rs              OS credential store — the only place a secret is written
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
- Wallet: import a known test phrase — the address must match what MetaMask shows for it.
- Wallet: send a testnet transfer, then edit the amount after the quote — the confirm button
  must go back to "Review transfer" rather than staying armed with the old fee.
- AI: open the AI tab with no key stored — it must offer the key form and never call anything.
- AI: research a coin, collapse and reopen the panel — the report comes back from disk with a
  "cached" footer and no second charge.
- AI: click a headline — it opens in the browser; a link that is not in a stored report is
  refused with a message rather than opened.
- Wallet: turn the price pill off — it disappears immediately and stays gone after a restart,
  and the tray icon opens the wallet.

## Known MVP limitations

- **Alerts only fire while the app is running.** Autostart is wired via
  `tauri-plugin-autostart` but left disabled by default (user decision, not a technical gap) —
  enable it in Settings if you want alerts to survive a reboot without a manual relaunch.
- **Work-area detection uses the monitor's full bounds**, not the OS-reported work area minus
  the taskbar (that needs Win32 GDI FFI). In practice this only matters for bottom-edge
  docking on a machine with a bottom taskbar; right/left/top docking are unaffected.
- **The wallet is EVM-only and deliberately small.** ETH and ERC-20 transfers on whichever chain
  the configured RPC serves — no swaps, no NFTs, no contract interaction beyond `transfer`, no
  hardware wallet support, and no address book.
- **Transaction history needs a free Etherscan API key.** There is no public endpoint that
  serves it; without a key the rest of the wallet works and the history tab says so.
- **AI research needs your own Anthropic key and spends money per call.** There is no free
  tier and no proxy: the widget ships the feature switched off, and every report is a button
  press. The answers are a model's opinion over web search — informative, not advice, and not
  a substitute for reading the sources it cites.
- No indicators — out of scope by design, see the original spec's "not-goals" section.

## License

MIT — see [LICENSE](LICENSE).
