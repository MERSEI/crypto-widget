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
| USDT→CZK rate           | FX host, primary + fallback pair  | 6h TTL                        |

Search filters the cached catalog (USDT pairs only) by symbol/base substring, sorts by quote
volume, and truncates to 50 — no network round-trip per keystroke.

### Backup venues

A pair can stop trading on Binance while the coin trades on elsewhere: `TONUSDT` sat in `BREAK`
status for weeks, and both the WS stream and `/ticker/24hr` kept serving the price of its last
trade as if it were live. Any snapshot older than `STALE_AFTER_MS` (60s) is therefore treated as
stale — it is still shown, marked, but it never reaches the spike buffer or the alert engine.

`market/backup.rs` sweeps the watchlist every 20s and asks OKX → Bybit → KuCoin → Gate, in that
order, for any symbol the primary feed has gone quiet on; the venue that answered is remembered
per symbol and tried first next time. A newly added pair takes the same route immediately instead
of waiting for the sweep. The venue that supplied a price rides along in `TickerSnapshot.source`
and is shown in the row. Requests to the four venues share a `WeightLimiter` (`market/ratelimit.rs`)
so a watchlist full of stale symbols can't turn into an unthrottled burst, and every failed request
is logged with the venue's name instead of being swallowed silently.

Binance is always the priority source, enforced in `MarketHub::apply_ticker`: a backup sweep can
take up to ~30s to walk its venue list end to end, so Binance may have already recovered for a
symbol by the time a backup answer comes back. A backup update is only applied if the currently
stored price for that symbol is missing, from another backup venue, or itself stale — a fresh
Binance price is never overwritten by a backup answer that raced against it. This is per-symbol,
not global: the chain above never falls back for the whole watchlist, only for the one pair
Binance has nothing live for.

No price aggregator sits in that chain on purpose: aggregators are keyed by ticker symbol, and
symbols get recycled — after Toncoin's rebrand to GRAM, CoinGecko's `TON` is Tokamak Network, so
a symbol lookup would have answered "TONUSDT = 0.27" with complete confidence.

## Alerts

Three rule kinds share one evaluation path, run on every incoming tick for the matching symbol:

- `price_above` — fires when the price *crosses* up through `value`
- `price_below` — fires when the price *crosses* down through `value`
- `spike` — `|change| >= value%` over `windowMinutes`, comparing the latest price to the oldest
  point still inside the window (`alerts/spike.rs`)

Level rules are edge-triggered, not level-triggered, and carry an `armed` flag for it:
`Some(true)` waits for a crossing, `Some(false)` means the level has been taken and the price has
to come back before it counts again, `None` is a rule that has not seen a price yet — its first
tick only arms it, so a level the market already sits beyond never fires on the spot. (That is
what made a frozen TONUSDT at 1.60 announce "crossed above 1.3".) A crossing during cooldown is
consumed, not deferred.

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
  the apply step (`snap_pill/panel_geometry`) is a no-op when the window is already docked,
  because repositioning emits another `Moved` and an unconditional apply would loop forever.
- Expand/collapse (`apply_pill/panel_geometry`) always calls `set_size`/`set_position`, unlike the
  edge-snap path above — skipping it on a stale `outer_size()` reading used to silently drop the
  resize, leaving the window pill-sized while the renderer had already painted the panel, fixable
  only by a restart. `AppState.geometry_lock` serialises every caller of either path (pill click,
  tray, blur autocollapse, drag-end snap) so two can't interleave their reads and writes.
- Expanding swaps pill geometry for panel geometry (`panelWidth × panelHeight`, default 380×520)
  against the same edge anchor.
- Blur auto-collapse re-checks focus after 220ms before acting: WebView2 briefly drops window
  focus whenever it spins up a native surface (opening a chart is the reliable trigger), which
  used to collapse the panel out from under a mid-click user. `pinned` disables auto-collapse
  entirely; always-on-top is unconditional.
- Work area is approximated by the monitor's full bounds — see the README's limitations.

## Wallet

`src-tauri/src/wallet/` is the only part of the app that can spend money, so the split that is
merely tidy elsewhere is load-bearing here: **the seed phrase never crosses the IPC boundary.**
A compromised or simply buggy renderer can ask for a transfer; it cannot sign one.

- `keystore.rs` — BIP-39 phrase, BIP-44 derivation at `m/44'/60'/0'/0/index`, stored in the OS
  credential store under its own namespace. The path is pinned to the one MetaMask and every
  hardware wallet use: anything else produces addresses the user's written-down phrase does not
  restore anywhere else. A phrase is checksum-validated before it is stored, because one swapped
  word still opens a perfectly real, permanently empty wallet.
- `chain.rs` — balances, fee quotes and the send path, over alloy. Amounts cross IPC as **strings**
  (a wei-scaled balance does not survive a JS `number`), and token decimals are always read from
  the contract rather than from settings.
- `etherscan.rs` — history over the v2 API. `status: "0"` means either "no transactions" or a real
  failure, and the two are told apart by message; flattening both into `[]` made a rejected key
  look like a brand-new wallet.
- `commands.rs` — the IPC surface, and the three checks that stand between a request and a
  broadcast: the destination is checksum-verified and refused if it is the zero address or the
  token's own contract, the amount is parsed against the asset's real decimals, and the fee the
  user approved is re-quoted before signing.

Sending is deliberately **two commands**. `quote_wallet_transfer` prices the exact transaction
`send_wallet_transfer` will rebuild; the renderer hands back the approved `maxCostWei`, and a
re-quote more than 25% above it is refused rather than signed. Gas moves between the dialog and
the click — an exact match would refuse honest transfers, and no check at all would let a spike
turn a two-dollar fee into a fifty-dollar one.

The wallet has its own window (`index.html?window=wallet`), like the futures terminal: the pill is
140×30, transparent and undecorated, which suits neither a balance table nor an address someone
has to read character by character. `wallet.widgetEnabled` makes the pill itself optional — with
it off, the tray icon opens the wallet instead of expanding a pill that is not there.

## AI research

`src-tauri/src/insights/` is the only module that returns an **opinion** rather than a fact, and
the only one where a single command costs the user money. Both properties drive the design.

- `ai.rs` — the Anthropic Messages API over raw HTTP (there is no official Rust SDK). Three
  failure modes it exists to get right: a server-tool turn can answer `stop_reason: "pause_turn"`
  with half a turn that has to be echoed back verbatim to continue; a *failed* web search arrives
  as HTTP 200 with an error **object** where the success case has a **list**, so parsing it as an
  empty result set would report "no sources" for a hard failure; and the web-search tool type is
  model-gated, so the current variant sent to an older model is a 400 rather than a downgrade.
- `coingecko.rs` — the measured half. Market cap, rank, supply, distance from the ATH and
  community sentiment, cached on disk and falling back to a stale copy on a 429 rather than
  failing the report. `pick_best` decides which listing a ticker means — an exact symbol match
  with the best market-cap rank — because dozens of dead tokens share a live asset's ticker.
- `commands.rs` — prompts, cache, sanitising, and the IPC surface.

Four rules hold the feature together:

1. **No call without a click.** There is no poll loop and no prefetch. Opening the tab, switching
   symbols and reopening the panel read the disk cache (`get_cached_insight` / `get_cached_scan`);
   only `research_coin` / `research_market` reach the model, and only `refresh: true` pays for an
   answer that is already on disk.
2. **One call at a time.** A `Mutex` in `commands.rs` refuses a second request instead of queueing
   it — a double-clicked button that bills twice is the failure it prevents. The renderer's `busy`
   flag is a convenience on top; the backend is the guarantee.
3. **Measured and argued stay separate.** `CoinInsight.analysis` is the model's; `.fundamentals`
   is CoinGecko's; `.sources` are harvested from the search-result blocks rather than from the
   model's prose, so the citation list cannot be fabricated. The panel renders them as three
   labelled blocks for the same reason.
4. **Every link is vouched for.** `open_insight_url` refuses any URL that is not present, field
   for field, in a stored report — the same rule `open_referral_url` follows, and with more force
   here, because every string in a report was written by a model or lifted from a search result.

Anything the model returns is clamped before it is stored: a score above 100, a 300-item idea
list, a headline with no URL. A missing section degrades one block of the card; it never fails a
call that has already been billed, which is why every field in `Analysis` carries a
`serde(default)`.

The Anthropic key lives in the OS credential store under the `anthropic` namespace via
`save_raw`/`status_raw` — one opaque string, not a key/secret pair. Filing it as a pair would
make `status` report a stored key as missing.

## Persistence

`ConfigState` loads `settings.json` from the Tauri app config dir on startup, keeps it behind an
`RwLock`, and a background task flushes it **at most twice a second** when dirty. Commands mutate
the struct and call `mark_dirty()` — no command writes to disk synchronously.

- Schema is versioned (`SETTINGS_VERSION = 4`; every added section is `#[serde(default)]`, so an
  older file gains defaults instead of failing to parse — a parse failure backs the file up and
  starts over, taking the watchlist and alerts with it); the Rust structs are `camelCase`-serialized and
  mirrored by `src/types/settings.ts`. **Changing one means changing the other.**
- A corrupt file is *backed up*, not deleted, and the app starts from defaults.
- `cache/` holds TTL envelopes (`saved_at` + value) for the catalog, FX rate, last tickers,
  CoinGecko lookups, and AI reports. The report TTL is the user's `insights.cacheTtlMin`, read at
  serve time rather than baked in at write time, so shortening the window expires answers that
  are already on disk.

## IPC contract

All commands are declared in `src-tauri/src/commands.rs`, registered in `lib.rs`, and wrapped
type-safely in `src/core/ipc/commands.ts`. Reads first, then mutations, then window control:

| Command                                  | Purpose                                        |
| ---------------------------------------- | ---------------------------------------------- |
| `get_settings` / `get_tickers` / `get_connection` / `get_expanded` | Initial hydration     |
| `search_pairs` / `get_klines` / `get_fx_rate`                      | On-demand market data |
| `add_watchlist_symbol` / `remove_watchlist_symbol` / `reorder_watchlist` / `set_pinned_symbol` | Watchlist |
| `set_display` / `set_chart_settings` / `set_notifications` / `set_autostart` | Settings   |
| `upsert_alert` / `delete_alert`          | Alert rules                                    |
| `start_drag` / `drag_ended` / `set_pin` / `toggle_expand` / `collapse_if_unpinned` | Window |
| `get_wallet_state` / `get_wallet_balances` / `get_wallet_history` | Wallet reads             |
| `create_wallet` / `import_wallet` / `reveal_seed_phrase` / `forget_wallet` | Seed lifecycle   |
| `set_wallet_account` / `set_wallet_network` / `set_wallet_widget_enabled` | Wallet settings   |
| `add_wallet_token` / `remove_wallet_token` / `set_etherscan_key` / `clear_etherscan_key` | Wallet setup |
| `quote_wallet_transfer` / `send_wallet_transfer` | Transfer, reviewed then confirmed           |
| `open_wallet_window` / `open_futures_window`     | Secondary windows                           |
| `get_insights_state` / `get_cached_insight` / `get_cached_scan` | AI reads — never call the model |
| `set_insights_settings` / `set_anthropic_key` / `clear_anthropic_key` | AI setup              |
| `research_coin` / `research_market`      | The paid calls, one click each                 |
| `open_insight_url`                       | Opens a link that is in a stored report        |

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
- `settings.pinnedSymbol` — which symbol the pill shows; `null` falls back to the first watchlist
  row (by `order`). Set from the ★ button on a watchlist row; cleared automatically if that
  symbol is removed from the watchlist.
- `alerts` — rule list, refreshed from the `alerts` event
- `settings` — the mirrored `AppSettings`
- `ui` — expanded/settings-open flags, connection status, FX rate
- `wallet` — wallet state, balances and history, each with its own loading/error slice: history
  needs an Etherscan key and balances do not, so a rate-limited history must not blank out
  balances that loaded fine
- `insights` — AI settings, the current coin report and the market scan, plus one `busy` flag
  (the backend serialises calls, so two spinners could never both be true). A late cache read for
  a symbol the user has moved away from is dropped rather than rendered under the wrong heading.

`features/` holds the visual units (pill + drag hook, watchlist rows and search dialog, chart
accordion with timeframe bar, alert editor and list, settings panel and status bar). `core/format/`
keeps price/percent/volume formatting pure and tested — significant-digit handling for sub-cent
prices is the kind of thing that silently regresses.

## Testing

| Layer          | Command             | Covers                                                                   |
| -------------- | ------------------- | ------------------------------------------------------------------------ |
| TypeScript     | `npm run test`      | Format helpers, ticker store selectors, the insights store's "no call without a click" rule |
| Rust           | `cargo test`        | Spike detector, alert engine decision logic, WS backoff, kline/ticker parsing (against captured real payloads), settings load/corrupt-recovery, AI report parsing and link vouching |

CI (`.github/workflows/ci.yml`) runs both suites on every push and PR, split by platform: the
frontend job (typecheck → vitest → `vite build`) on Linux, the Rust job (`clippy -D warnings` →
`cargo test`) on Windows. The Rust job builds the renderer first — `tauri::generate_context!`
resolves `frontendDist: ../dist` at compile time, so without a `dist/` directory the crate does
not even typecheck, let alone test. There is no `cargo fmt --check`: this source is hand-wrapped
wider than rustfmt's defaults, so the gate would demand a whole-tree reformat instead of catching
anything real.

The parser tests deliberately feed real API payloads including malformed and extra-column rows:
Binance adds trailing kline columns over time, and `"NaN".parse::<f64>()` succeeds — both were
live panic paths before they were tests.
