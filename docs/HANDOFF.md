# Tidemail handoff

## Phase 0 status

- Rust workspace, SQLite schema/migrations and indexed read queries are in place. Migrations refuse a database newer than the build.
- `fakegen` creates a reproducible 3-account mailbox with 50,000 messages and hostile HTML fixtures.
- Tauri 2 app shell, tauri-specta command/event registration, tray, hide-on-close, single-instance handling and first-paint reveal are wired.
- The main window is built in `setup` with `on_navigation` / `on_new_window`: non-app URLs are cancelled and `http`/`https`/`mailto` open in the default app.
- The `tidemail-cid` scheme is registered and serves `inline_blobs` at `http://tidemail-cid.localhost/<message_id>/<percent-encoded cid>` (`tidemail_render::CID_ORIGIN`). The app CSP allows it, `https:` images and `data:` fonts, because the reader's srcdoc iframe inherits the app CSP.
- DB commands run off the main thread (`#[tauri::command(async)]`).
- React/Vite/Tailwind skeleton in `src/features/{shell,list,theme}` and `src/lib`; `reader` and `search` folders are reserved. System-following dark/light theme on `:root`. The list pages through the inbox with keyset cursors (100 rows per call).
- `bun run bench` runs both parts: the Rust bench and the E2E bench (`e2e/bench.ts`).
- The CLI accepts `--db <path>`; default is `%LOCALAPPDATA%\Tidemail\mail.db`.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `bun run lint`, `bun run typecheck` (`tsc -b`), `bun run build` pass.

### Bench (bench.db, 2026-11-02)

| Metric | Result | Budget |
|---|---|---|
| open db + first inbox page (Rust) | 4.0 ms | 100 ms |
| list page p95 (Rust) | 0.18 ms | 10 ms |
| thread open + body p95 (Rust) | 0.16 ms | 15 ms |
| search, action write | SKIP (Phase 1) | |
| **cold launch, worst of 3 (E2E)** | **705 ms (543–705) FAIL** | 400 ms |
| scroll dropped frames (E2E) | 0 / 600 | ≤ 2 % |
| scroll blank frames (E2E) | 0 | 0 |
| j/k open to paint | SKIP (needs reader) | 50 ms |

Cold launch breakdown: process start to page navigation start is about 390–460 ms (WebView2 environment start; the CDP debug port adds some). Navigation to first paint is about 175 ms, with about 60 ms between script load and the first `list_threads` call. Cold launch is measured from the start of `run()`, not from process creation.

## Phase 1 status (branch `phase1`, not pushed)

- Five agents built renderer, search, UI shell, Gmail sign-in/client and sync engine on `phase1-<area>` branches; all merged into `phase1` in spec order. Per-area notes: `docs/handoff/{renderer,search,shell,gmail,sync}.md`.
- Applied by the main thread: `set_sender_images` and `open_attachment` IPC commands; migration 0002 (`threads(last_date DESC, id DESC)` for search); sync saves attachments through `tidemail_render::save_attachment_in` (Mark-of-the-Web); `src-tauri/src/gmail_adapter.rs` maps `tidemail_gmail` to the sync traits (`GmailApi`, `GmailProvider`), and the engine starts with it. `config.toml` is read lazily on first sign-in/API call, so the app runs on `bench.db` without it.
- `cargo test --workspace`, clippy `-D warnings`, `bun run lint`, `bun run typecheck`, `bun run build` pass.
- Not checked: the UI in a running app by a person (only build checks and the E2E bench), Credential Manager writes (`keyring` 4.x), anything against real Gmail.

### Bench (bench.db, 2026-11-02, after the Phase 1 merge)

| Metric | Result | Budget |
|---|---|---|
| open db + first inbox page (Rust) | 32.6 ms (includes migration 0002 on first open) | 100 ms |
| list page p95 (Rust) | 0.16 ms | 10 ms |
| thread open + body p95 (Rust) | 1.08 ms | 15 ms |
| search per keystroke p95 (Rust) | 11.3 ms (max 73.5 ms) | 30 ms |
| action write p95 (Rust, local) | 0.72 ms | 16 ms |
| **cold launch, worst of 3 (E2E)** | **538 ms (433–538) FAIL** | 400 ms |
| scroll dropped / blank frames (E2E) | 0 / 600, 0 | ≤ 2 %, 0 |
| j/k open to paint p95 (E2E) | 7 ms (reader header; iframe paint not awaited) | 50 ms |

### Phase 1 gaps closed (2026-11-02)

- Prefetch: opening a thread calls `prefetch_threads` for the next N threads in the list (N = 5 / 10 / 20 / 25 / 50, default 10, set in the Ctrl+K palette, stored in `localStorage` `tidemail.prefetch`). `prefetch_threads` replaced the unused `prefetch_bodies` IPC command; engine prefetch queue is 512 ids.
- "Move to…" (`v`, also in the palette): picks a user label of the thread's account; the action adds it and removes `INBOX` in one `threads.modify`.
- Re-sanitize job: each account loop, after backfill and the body pass, refetches (`format=full`) bodies with `sanitizer_version < SANITIZER_VERSION`, newest first, 50 per batch, and stores them sanitized again (FTS refreshed). Migration 0003 indexes `bodies(sanitizer_version)`.
- Known gaps per area are listed in each `docs/handoff/<area>.md`.

## UI switch to GPUI (2026-11-06)

- Decision and startup findings: `docs/adr/0001-native-ui-with-gpui.md`. `docs/SPEC.md` is updated.
- `phase1` was fast-forwarded into local `main` (`a8cf15e`, not pushed). Work continues on branch `gpui`.
- Pins: `gpui-kit =0.9.0` (`gpui-component` 0.9.0, `gpui-pre` 0.3.11), `gpui-webview =0.9.0` (`lb-wry` 0.53.3), `tray-icon =0.26.1`.
- `vendor/gpui-pre-windows`: stock 0.3.11 plus `vendor/gpui-pre-windows-parallel-dx.patch` (DirectX devices on a thread during DirectWrite start). Re-apply on upgrade.

## Phase 1G-A status (branch `gpui`)

- Removed: `src-tauri/`, `src/`, `e2e/`, `scripts/bench.ts`, all JS tooling, `specta` derives in `tidemail-db`. `View` derives `PartialEq, Eq`.
- `crates/app` (binary `tidemail`):
  - `main.rs`: args (`--db`, `--bench <report.json>`), single instance, preload thread (DB open + accounts + labels + first inbox page + `Engine::start_with`) running while GPUI starts, window (cloaked until frame 1), hide on close, tray, event loop.
  - `single_instance.rs` (mutex + pipe, per database path), `tray.rs`, `win.rs` (HWND, cloak, show/hide, process start time), `events.rs` (`AppEvent` channel; `EventTx` is the sync `Notifier`), `data.rs` (`Db` read pool on the background executor, `Preload`), `settings.rs` (`settings.json` next to the DB: theme, prefetch), `theme.rs`, `format.rs`, `gmail_adapter.rs` (moved from `src-tauri`, opener is `tidemail_gmail::open_in_browser`).
  - `actions.rs`: every action, default keys, `SHORTCUTS` table for the palette. Key context `Tidemail` on the shell root.
  - `state.rs`: `AppState` entity + `StateEvent` (frozen for 1G-B, see below).
  - `features/shell`: title bar (search slot), sidebar (unified mailboxes, accounts with sub-mailboxes, user labels, status line), 3 panes. Handles j/k/Enter/g i/e/#/s/u.
  - `features/list`: `uniform_list`, 64 px rows, account dots in unified views, keyset paging (100 rows, loads 30 rows ahead), refresh once per burst of `DataChanged`, opens the next row when the opened thread leaves the view, prefetch of the next N threads on open.
  - `features/reader`: stub (native subject + per-message sender/date/snippet). `features/search`: title-bar slot stub. `features/status`: one-line stub. `features/palette`: empty.
  - `bench.rs`: `--bench` runs 100 j/k steps (press until `Reader::shown()` equals the opened thread), then 600 scroll frames (192 px per frame), writes JSON, quits.
- `crates/bench`: new `bench-all` bin (Rust bench, then 3 app launches on DB copies in `%TEMP%	idemail-bench`).
- Checked by hand (scripted): window paints, j opens rows and marks them read, close hides (process stays), second launch exits in ~40 ms and shows the first window, title-bar controls render (needs `with_assets`).
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release` pass.
- `#[allow(dead_code)]` on `mod actions/format/settings/state` in `main.rs`: API waiting for 1G-B. Remove when 1G-B lands.

### Bench (bench.db, 2026-11-06, two runs)

| Metric | Result | Budget |
|---|---|---|
| open db + first inbox page (Rust) | 28 ms | 100 ms |
| list page p95 (Rust) | 0.10–0.46 ms | 10 ms |
| thread open + body p95 (Rust) | 0.13–1.57 ms | 15 ms |
| search per keystroke p95 (Rust) | 11.1–14.9 ms | 30 ms |
| action write p95 (Rust, local) | 0.48–0.93 ms | 16 ms |
| **cold launch, worst of 3 (app)** | **281 / 297 ms** (256–297) | 400 ms (target 300) |
| j/k open to paint p95 (app, reader stub) | 4.4 ms | 50 ms |
| scroll dropped / blank frames (app) | 0–4 / 1800, 0 | ≤ 2 %, 0 |

The first launch after a fresh build measured 546 ms once (new exe: disk cache, Defender scan); repeated runs stay under 300 ms.

## Frozen for Phase 1G-B

- `AppState` fields: `db`, `engine`, `accounts`, `labels`, `unread`, `nav`, `selected`, `reading`, `status`, `settings`. Methods: `navigate`, `select`, `open`, `close_reader`, `clear_selection`, `spec`, `unread(view)`, `apply(kind, ids, Some(text))` (emits `ActionDone { text, action_ids }` / `ActionFailed`), `undo`, `prefetch`, `push_overlay` / `close_overlay` / `overlay_open` / `top_overlay`, `set_theme`, `set_preset(preset, window, cx)`, `set_prefetch`.
- `StateEvent`: `NavChanged`, `SelectionChanged`, `ReadingChanged`, `DataChanged`, `AccountsChanged`, `StatusChanged`, `OverlaysChanged`, `SettingsChanged`, `ActionDone`, `ActionFailed`.
- `Overlay`: `Palette`, `LabelPicker`, `MovePicker`, `GoPicker`, `Toast`.
- `actions.rs` action list and key context. `ThreadList::{rows, current, open_index, open_current, select_next, select_prev, row_height}`. `PresetSpec`, `theme::colors(cx)`, `ui.rs` primitives. `Reader::shown()` (bench contract).
- Changes to these go in a "Requests" section of this file; the main thread applies them.

## UI presets (decided 2026-11-11, built in 1G-U1)

Decision: `docs/adr/0002-layout-presets.md`. Three presets (Tide default, Command, Paper), each with dark + light palettes, chosen in the palette. `docs/SPEC.md` is updated. Mockups were shown in the planning thread only; the ADR table is the reference.

## Phase 1G-U1 (main thread, before 1G-B): done 2026-11-19

Order matters: each step builds and passes clippy on its own.

1. **Contract.** `preset.rs` (`Preset`, `PresetSpec`, the three specs). `Settings::preset` (default Tide). `AppState`: `reading`, `select`, `close_reader`, `set_preset`; `open` sets `reading`. `StateEvent::ReadingChanged`, `Overlay::GoPicker`, actions `GoLabel` (`g l`) and `CloseReader` (`Esc` while reading). `--preset <name>` in `main.rs`.
2. **Theme.** `theme.rs`: `Palette` per preset and mode, written over the `gpui-component` `Theme` global after `Theme::change`. Check first that 0.9.0 lets the colours be set; if not, read colours from our own global. Font: Segoe UI Variable, 13 px base; Paper uses Georgia for subject and list title.
3. **`ui.rs`.** Icon button, key hint, chip, avatar (initial + colour from a hash of the sender), section heading. Icons from the `gpui-component` set; drop the `▣ ☆ ↗ ◇` glyphs.
4. **Navigation.** `features/shell/nav.rs`: `items()` model. New `read::unread_counts` in `crates/db` (per `label_key`, unified and per account; check with `EXPLAIN QUERY PLAN`), reloaded once per `DataChanged` burst. Renderers: `sidebar.rs` (exists), `tabs.rs`, `rail.rs`. Remove the "Compose (Phase 3)" box.
5. **Rows.** `features/list/row.rs`: `two_line`, `one_line`, `three_line`. `ROW_HEIGHT` becomes `spec.row_height`. No row borders; selected row is a tinted, rounded fill; unread is an accent dot + bold; account colour is a 2 px rail in unified views. `select_next` / `select_prev` call `open` or `select` per the ADR.
6. **Shell.** Layout from `spec.nav` and `spec.reader`. List header (view title + unread count) in Tide and Paper. Command: hint bar, reader shown only while `reading`. Pane widths: Tide 208 / 400 / rest; Paper 44 / 320 / rest.
7. **Bench.** `--bench` honours the preset: scroll step is 3 rows of the active preset; Command presses Enter once before the j/k steps. `bench-all` loops the three presets.
8. Checks: clippy, tests, release build, `bench-all`. Watch cold launch (font and icon assets); budget has about 100 ms of margin.

Until the commands agent adds the palette entry, switch presets with `--preset` or `settings.json` (`"preset": "tide" | "command" | "paper"`).

### 1G-U1 progress (2026-11-19)

1. Contract: done. `preset.rs` (`Preset`, `PresetSpec` with pane widths, `display_font: Option`), `Settings::preset`, `AppState::{reading, spec, select, close_reader, set_preset}`, `open` sets `reading`, `navigate` / `clear_selection` close the reader. `StateEvent::ReadingChanged`, `Overlay::GoPicker`, actions `GoLabel` (`g l`, not handled until the commands agent builds the picker) and `CloseReader` (`escape` in context `Tidemail && reading`; the shell root adds `reading`). `ThreadList::open_current` (Enter). `--preset <name>` overrides the setting for the run (it is saved only if another setting changes).
2. Theme: done. `gpui-component` 0.9.0 lets every colour be set (`Theme::update` after `Theme::change`). `theme::Palette` (hex) per preset and mode lives in `PresetSpec::{dark, light}`; `theme::apply(pref, preset, window, cx)` writes it over the component theme and sets the `theme::Colors` global (selection tint, hover, `paper` / `paper_fg` for mail bodies). `set_preset` takes `window` and re-applies the theme. Font `Segoe UI Variable Text`, 13 px on the shell root (the window rem stays 16 px: `base::Root` does not set it, so `ui::TEXT_*` are px).
3. `ui.rs`: done. `icon`, `icon_button` (tooltip), `key_hint`, `chip`, `count`, `avatar` (FNV hue), `heading`, `dot`. `assets.rs` adds Lucide `archive, arrow-left, file-pen-line, folder-input, mail, send, tag, trash, x` (in `crates/app/assets/icons`, ISC licence) over the `gpui-kit` default set. Sidebar glyphs replaced by icons.

4. Navigation: done. `features/shell/nav.rs` (`items()`, `MAILBOXES`, `toggle_account` for account dots: filters the current view to one account, again to unfilter). `read::unread_counts` in `crates/db` with migration 0004 (partial index `threads(id) WHERE unread_count > 0`; `CROSS JOIN` fixes the join order, test checks the plan). Loaded in the preload and reloaded with accounts/labels once per `DataChanged` burst. Counts shown for Inbox and user labels only. `sidebar.rs` (208 px, no Compose box, status line at the bottom), `tabs.rs` (title bar: mailbox tabs, account dots, `g l` hint), `rail.rs` (44 px icons with an unread dot, account dots).
5. Rows: done. `features/list/row.rs`: `two_line` (Tide), `one_line` (Command), `three_line` (Paper, avatar, subject in Georgia). Inset rounded selection fill, no borders, accent unread dot + semibold, 2 px account rail in unified views. `ROW_HEIGHT` removed; height from the spec.
6. Shell: done. Tide: sidebar / list 400 / reader. Paper: rail / list 320 (search slot on top, status line at the bottom) / reader. Command: tabs + search in the title bar, list or reader full width, hint bar with status at the bottom. List header (title + "N unread") in Tide and Paper. `search::slot` now fills its container (title bar gives it 360 px).
7. Bench: done. `bench-all` runs 3 launches per preset with `--preset`; scroll step is 3 rows of the preset; Command opens the first thread before the j/k steps and closes the reader before scrolling.
8. Checks: clippy `-D warnings`, `cargo test --workspace`, release build, `bench-all` pass (2026-11-19, table below). Checked by screenshot: Tide dark, Command light/dark (list, reading after Enter, back after Esc), Paper light/dark.

Not done in 1G-U1: `GoLabel` / `GoPicker` (the commands agent builds the picker; until then tabs and the rail reach labels only through account filtering), `ui::icon_button` and `ui::chip` are unused until the reader agent adds the toolbar (`#[allow(dead_code)]` on each).

### Bench (bench.db, 2026-11-19, after 1G-U1, second run on the same exe)

| Metric | Tide | Command | Paper | Budget |
|---|---|---|---|---|
| cold launch, worst of 3 | 368 ms (282–368) | 327 ms (303–327) | 310 ms (296–310) | 400 ms |
| j/k open to paint p95 | 4.4 ms | 4.3 ms | 4.3 ms | 50 ms |
| scroll dropped / blank | 0 / 1800, 0 | 0 / 1800, 0 | 0 / 1800, 0 | ≤ 2 %, 0 |

Rust bench: open + first page 2.2 ms, list page p95 0.17 ms, thread open p95 0.14 ms, search p95 12.8 ms, action write p95 0.9 ms. The first run after a rebuild had one Tide launch at 525 ms (new exe, as in 1G-A). Cold-launch margin is now about 30–90 ms, not 100 ms: watch it when 1G-B adds the webview and 1G-U3 adds fonts.

## Next: Phase 1G-B (three Sonnet agents in worktrees, merge order reader, search, commands)

Additions to the briefs below, from the presets decision:

- **All:** read sizes and colours from `PresetSpec` / the theme; use `ui.rs` primitives; no hard-coded layout.
- **Reader:** `ReaderPlacement::Replace` (Command) means the view exists but is on screen only while `state.reading`; hide the webview otherwise. Body column max 760 px, centred, on the light paper surface in both modes. Toolbar (archive, trash, star, label, move) with the key in each tooltip. Older messages collapsed to one line, last one open.
- **Search:** the shell places `search::slot` (title bar in Tide, replaces the tabs in Command, top of the list in Paper). Results use the row renderer of the active preset.
- **Commands:** palette entries "Layout: Tide / Command / Paper" (`set_preset`); `GoPicker` (all labels, `g l`) sharing the list widget of the label picker; toast host at the bottom of the list pane (Tide, Paper) or in the hint bar (Command), never over the webview.

Then **Phase 1G-U2** (main thread): visual pass by screenshot, 3 presets × 2 modes × (list, reader, palette, empty, error), then the full bench.

1. **Reader** (`features/reader`): `gpui-webview` host created after the first frame and kept warm; `with_javascript_disabled`, no devtools/context menu/zoom/drag-drop; `load_html` of `bodies.html_sanitized` with the CSP meta tag; `tidemail-cid` custom protocol from `tidemail_render::load_cid`; navigation and new-window handlers open `http`/`https`/`mailto` in the default app and cancel everything else; "Load images" per sender (`write::set_sender_images`); plain-text fallback; attachments (download via `engine.download_attachment`, open with the default app); hide the webview while `overlay_open()`; `focus_parent()` after clicks; `shown()` true only after the webview content loaded (bench must include the body paint).
2. **Search** (`features/search`): `gpui-component` input in the title-bar slot, `/` focuses it, per-keystroke `tidemail_search::search` on the background executor (drop stale results), results list replacing the thread list while a query is active, `<mark>` snippets rendered as highlighted runs, Escape clears.
3. **Commands** (`features/palette`, `features/status`): Ctrl+K palette (actions + `SHORTCUTS`, theme System/Light/Dark, prefetch 5/10/20/25/50), label picker (`l`) and move picker (`v`), undo toast (5 s, Ctrl+Z) from `ActionDone`, error toast from `ActionFailed`, status area ("Synced 12 s ago" / "Offline" / "3 actions pending" / "1 action failed" with Retry / Dismiss).

Then Phase 2 as before.

## Decisions (2026-11-02)

- Opening a thread marks it read (silent, no undo toast; `u` marks it unread again).
- `u` toggles read/unread.
- Spam and trash are synced (`includeSpamTrash=true`). No sidebar views for them (spec does not list them); search still includes them.

## Open decisions


- Google OAuth client setup is required before account sync can be exercised.
- Phase 3 compose/editor and offline draft behavior remain intentionally undecided.
- Phase 4 projects/classification remain intentionally undesigned.

## After 1G-U2: colour themes (planned 2026-11-17, nothing built yet)

Plan: `docs/design/themes-plan.md` (Phase 1G-U3). Eight built-in themes plus a
custom accent the user picks, from which the rest of the palette is derived,
and a choice of eleven interface and ten code fonts (defaults Inter and
Obel Mono, replacing Segoe UI Variable), with a "Hide secrets in code"
switch (Obel Mono's masking).
Colour becomes separate from the layout preset; ADR 0003 will amend ADR 0002.
