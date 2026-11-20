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
- `vendor/gpui-pre-windows`: stock 0.3.11 plus `vendor/gpui-pre-windows-parallel-dx.patch` (DirectX devices on a thread during DirectWrite start) and `vendor/gpui-pre-windows-dcomp-not-topmost.patch` (1G-B: DirectComposition target not topmost, so the reader WebView2 child window draws above GPUI). Re-apply both on upgrade.

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

## Phase 1G-B status (branch `phase1g-b`, done 2026-11-20)

- Three Sonnet agents built reader, search and commands on `phase1g-b-{reader,search,commands}` (from `f90fa48`); merged into `phase1g-b` in that order. Area notes: `docs/handoff/{reader,search,commands}-1g.md`.
- Merge fixes (main thread): one `Dismiss` handler on the shell (closes the top overlay, else clears an active search query); `Shell::current_row` gives pickers and `act` the search result row while a query is active; removed the `allow(dead_code)` on `AppState::overlay_open`.
- Reader fixes after the first release bench (Command crashed; j/k p95 103–190 ms):
  - **Crash:** WebView2 creation runs a nested message loop (`webview2_com::wait_with_pump`). Built inside an `on_next_frame` callback, it ran GPUI tasks while `App` was borrowed (`RefCell already borrowed`). `host::build` now runs in a spawned task between updates and the result is wrapped with `gpui_webview::WebView::new`. Rule: never call WebView2 creation (or anything that pumps) inside a GPUI update.
  - **Per-frame WebView2 calls:** `gpui-webview`'s element calls `put_Bounds` and `put_IsVisible` on every prepaint. The reader draws its own `canvas` that sets bounds only when they change; visibility is `Reader::sync_visibility` only.
  - **DirectComposition kept:** the reader agent had set `GPUI_DISABLE_DIRECT_COMPOSITION=1`. The real cause is `CreateTargetForHwnd(hwnd, topmost = true)` in GPUI, which covers child windows. Patched to `false` in the vendored crate; the env var is gone.
  - **j/k:** a WebView2 navigation per step costs 70–190 ms. The first document, and every change of image policy, is still a full `load_html` with the CSP meta tag. Later bodies with the same policy are swapped in with a host `ExecuteScript` (`page::swap_script`: `body.innerHTML`, scroll to top, returns the nonce). Page scripts stay off; the meta CSP stays in force after the DOM changes. `shown()` waits for the script callback (or the page-load event after a navigation). Settings changes force a full navigation (page colours live in the head).
- Checked by screenshot (Tide dark, release): j swaps the body, the `cid:` image loads after a swap.
- `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass.

### Bench (bench.db, 2026-11-20, second run on the same exe)

| Metric | Tide | Command | Paper | Budget |
|---|---|---|---|---|
| cold launch, worst of 3 | 278 ms (272–278) | 276 ms (273–276) | 290 ms (271–290) | 400 ms |
| j/k open to paint p95 (incl. webview body) | 8.4 ms | 6.7 ms | 8.0 ms | 50 ms |
| scroll dropped / blank | 0 / 1800, 0 | 0 / 1800, 0 | 0 / 1800, 0 | ≤ 2 %, 0 |

Rust bench: open + first page 1.9 ms, list page p95 0.12 ms, thread open p95 0.23 ms, search p95 10.4 ms, action write p95 0.48 ms. The first run after a rebuild again had one Tide launch at 507 ms (new exe). Earlier the same day the machine was busy: the 1G-U1 exe itself measured up to 333 ms cold and dropped 600–827 scroll frames, so compare runs on a quiet machine only. A startup trace showed `main` is reached at ~25 ms and the first frame at ~280 ms, as in 1G-U1: WebView2 is not on the cold-launch path.

### Not checked after 1G-B (all checked in 1G-U2, see below)

- Overlays (palette, pickers) and toasts in 3 presets × 2 modes; that the webview hides under them; focus return after Esc.
- Search by real typing: `/` leaves no "/" in the input; Esc/Enter return focus; highlight colours.
- Reader: attachment download/open, "Load images" click, link open in the browser, Paper light.
- Ctrl+K and Ctrl+Z do nothing while the search input has focus (context `TidemailSearch` has no bindings).

## Phase 1G-U2 status (branch `phase1g-u2`, done 2026-11-20, commit b6a95dd)

Visual pass by screenshot: 3 presets × 2 modes × 17 states (list, reader, palette, Esc back, label / move / go pickers, toast, undo, search results, Ctrl+K in search, Esc from search, j after Esc, no results, Drafts via `g l`, odd query `"a*(`, Spam). Scripts: `scripts/ui-shots/` (see "Screenshot recipe" below).

Fixed:

- **Pickers typed into the shell.** While a picker's input had focus, the shell's single-key bindings still matched (`Tidemail` is an ancestor context): typing "drafts" in Go to label also starred the thread (`s`), and `e` would archive. The shell root context is now `TidemailPicker` while a picker is open, with only up / down / Enter / Esc / Ctrl+K bound (`actions::PICKER_CONTEXT`).
- **Ctrl+K in search** opens the palette (`ctrl-k` bound in `TidemailSearch`). Ctrl+Z stays unbound there on purpose: it is the input's own undo.
- **Esc peels one layer** (`CloseReader` handler): overlay, then the search query, then the reader. Before, with the reader open (always in Tide / Paper), Esc closed the reader and left the query.
- **Command: `/` while reading** closes the reader, so the results (which take the reader's place) are visible.
- **Search snippets** carried body line breaks: rows grew a third line (Tide) or overlapped the next row (Paper). `snippet::parse` collapses whitespace runs; test added.
- **Picker panel** always opened at full height (the backdrop stretched it); the last row drew past the panel edge. Backdrop `items_start`, list capped at 10.5 rows (`VISIBLE_ROWS`).
- **Reader:** header and body share one centred 760 px column (Command's full-width reader had the subject at the window edge); body has a 1 px border (paper vanished on Paper light); attachment chip had no icon (`download.svg` is not in the gpui-kit default set; Lucide `paperclip.svg` added); mail CSS `overflow-wrap:anywhere` broke table cells inside words ("Pric e", "$45. 40"), now `break-word`; `color-scheme:light` so the WebView2 scrollbar matches the paper surface when Windows is dark.
- **Focus after a click in the mail body** did not return: `SetFocus(parent)` inside WebView2's `GotFocus` is undone when WebView2 finishes the click. It now runs from a zero-delay thread timer (`host::refocus`). j works after a click (checked).
- **Paper:** status line at the bottom of the list has a top border (the cut-off last row ran into it).
- **Guard:** `assets::tests::every_icon_in_the_source_loads` fails when an `"icons/….svg"` literal in `src/` is missing from the asset source (missing icons draw nothing, with no error).

Checked and fine as is: webview hides under every overlay (the paper area shows dimmed under the backdrop); focus returns to the shell after Esc from pickers and search (j works); `/` leaves no "/" in the input; highlight colours in all presets; toasts at the bottom of the list (Tide, Paper) and in the hint bar (Command); "Load images" click hides the button and reloads the body; "View your order" opens the default browser (window "fabrikam.example") and the reader stays; attachment click without `config.toml` shows "Download failed: config: cannot read …" under the sender (the open path itself still needs real Gmail); "N actions failed · Retry · Dismiss" status in red in all presets; "No results" and empty reader ("Select a thread") states.

Checks: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass.

### Bench (bench.db, 2026-11-20, after 1G-U2, second run on the same exe)

| Metric | Tide | Command | Paper | Budget |
|---|---|---|---|---|
| cold launch, worst of 3 | 327 ms (299–327) | 343 ms (299–343) | 327 ms (305–327) | 400 ms |
| j/k open to paint p95 | 10.8 ms | 9.0 ms | 10.8 ms | 50 ms |
| scroll dropped / blank | 0 / 1800, 0 | 0 / 1800, 0 | 0 / 1800, 0 | ≤ 2 %, 0 |

Rust bench: open + first page 2.1 ms, list page p95 0.12 ms, thread open p95 0.13 ms, search p95 11.5 ms, action write p95 0.66 ms. The first run (new exe) had one Tide launch at 540 ms and 19 dropped scroll frames, as before. Cold launch is ~40–60 ms above the 1G-B numbers (278–290 ms); no change in 1G-U2 is on the startup path, and the machine sat at ~15 % CPU before the run, so treat the margin as ~55 ms until a quiet-machine rerun says otherwise.

### Screenshot recipe (1G-U2)

`scripts/ui-shots/drive.ps1` (run from the repo root, pwsh 7 works): copies `bench.db` to `%TEMP%\tidemail-shots\db` (`-Fresh`; otherwise reuses it, so actions accumulate), writes `settings.json` (preset, theme), starts the exe, resizes the window to 1440 × 900 after it settles, brings it to the front and runs `-Steps "wait:ms|send:<SendKeys>|text:<literal>|click:x,y|shot:name"`. Keys go through `SendKeys` (real input, so Ctrl+K / Ctrl+Z work); the window must stay in front, so do not use the machine while it runs, and never run two passes at once (two threads did on 2026-11-20 and their keys crossed). Shots use `PrintWindow(PW_RENDERFULLCONTENT)`. `all.ps1` is the full 17-state pass per combination; `sheet.ps1` makes a contact sheet; `crop.ps1` zooms; `titles.ps1` lists window titles (link-open check).

## Phase 1G-B plan (as briefed; done)

Additions to the briefs below, from the presets decision:

- **All:** read sizes and colours from `PresetSpec` / the theme; use `ui.rs` primitives; no hard-coded layout.
- **Reader:** `ReaderPlacement::Replace` (Command) means the view exists but is on screen only while `state.reading`; hide the webview otherwise. Body column max 760 px, centred, on the light paper surface in both modes. Toolbar (archive, trash, star, label, move) with the key in each tooltip. Older messages collapsed to one line, last one open.
- **Search:** the shell places `search::slot` (title bar in Tide, replaces the tabs in Command, top of the list in Paper). Results use the row renderer of the active preset.
- **Commands:** palette entries "Layout: Tide / Command / Paper" (`set_preset`); `GoPicker` (all labels, `g l`) sharing the list widget of the label picker; toast host at the bottom of the list pane (Tide, Paper) or in the hint bar (Command), never over the webview.


1. **Reader** (`features/reader`): `gpui-webview` host created after the first frame and kept warm; `with_javascript_disabled`, no devtools/context menu/zoom/drag-drop; `load_html` of `bodies.html_sanitized` with the CSP meta tag; `tidemail-cid` custom protocol from `tidemail_render::load_cid`; navigation and new-window handlers open `http`/`https`/`mailto` in the default app and cancel everything else; "Load images" per sender (`write::set_sender_images`); plain-text fallback; attachments (download via `engine.download_attachment`, open with the default app); hide the webview while `overlay_open()`; `focus_parent()` after clicks; `shown()` true only after the webview content loaded (bench must include the body paint).
2. **Search** (`features/search`): `gpui-component` input in the title-bar slot, `/` focuses it, per-keystroke `tidemail_search::search` on the background executor (drop stale results), results list replacing the thread list while a query is active, `<mark>` snippets rendered as highlighted runs, Escape clears.
3. **Commands** (`features/palette`, `features/status`): Ctrl+K palette (actions + `SHORTCUTS`, theme System/Light/Dark, prefetch 5/10/20/25/50), label picker (`l`) and move picker (`v`), undo toast (5 s, Ctrl+Z) from `ActionDone`, error toast from `ActionFailed`, status area ("Synced 12 s ago" / "Offline" / "3 actions pending" / "1 action failed" with Retry / Dismiss).

Then Phase 2 as before.

## Decisions (2026-11-02)

- Opening a thread marks it read (silent, no undo toast; `u` marks it unread again).
- `u` toggles read/unread.
- Spam and trash are synced (`includeSpamTrash=true`). No sidebar views for them (spec does not list them); search still includes them.

## Open decisions

- WebView2 user data folder: still the wry default (next to the exe). Move it under `%LOCALAPPDATA%\Tidemail` before the app is installed anywhere else; decide whether the cache is kept between runs.

- Google OAuth client setup is required before account sync can be exercised.
- Phase 3 compose/editor and offline draft behavior remain intentionally undecided.
- Phase 4 projects/classification remain intentionally undesigned.
- From 1G-U2:
  - Ctrl+Z while the search input has focus is the input's undo, not the toast's undo. Keep, or bind `Undo` in `TidemailSearch`?
  - Attachment errors show the raw engine text ("Download failed: config: cannot read C:\...\config.toml …"). Fine for development; give it a user-facing message before release.
  - Under an overlay the webview is hidden and the paper area shows blank (dimmed). A snapshot of the page would look better; WebView2 `CapturePreview` costs a capture per overlay open. Not done.
  - One unexplained stray Archive during the scripted pass (Tide light, after a run that clicked "Load images" and pressed k): not reproduced in a replay or in any later run. Cause unknown (scripted input runs through the real keyboard queue, so outside input can reach the app); watch for it.
- From 1G-U3:
  - Account dot colours are unchanged: amber `#f5a524` is 1.8:1 on Daylight / Linen (and on the light presets), violet `#6e56cf` 2.9:1 on Fjord. Each dot sits next to the account name, so not fixed; pick darker light-mode variants if it matters.
  - A red custom accent looks like danger (as in Forkline). Allowed; no warning shown.
  - Text size is not a setting yet (`theme::FONT_SIZE`).
  - The preset colour tweaks above change ADR 0002 values by a few percent of lightness; ADR 0003 notes it.

## Phase 1G-U3 status (2026-11-20, branch `phase1g-u3`)

Colour themes, custom accent and the two default fonts, per `docs/design/themes-plan.md` and ADR 0003 (amends ADR 0002). Board values as built: `docs/design/themes-canvas-values.md`.

- **Colour choice per mode** (`settings.rs`): `colorsDark` / `colorsLight` = `{"kind": "preset"}` (default), `{"kind": "theme", "name": "<theme>"}` or `{"kind": "accent"}`, plus `accent: {hue, chroma: "normal" | "muted", tint}`. Settings load field by field: an unknown value falls back to its default and keeps the rest.
- **Themes** (`theme/themes.rs`): Midnight, Graphite, Fjord, Forest, Rosé, High contrast (dark); Daylight, Linen (light). Board HSL values; the board has no danger colour, so each mode gets one. Changes from the board: Midnight text on accent is dark (white was 3.4:1); Daylight and Linen stars are darker (38 % L) to reach 3:1.
- **Custom accent** (`theme/derive.rs`): OKLCH from hue, chroma and tint; text, muted text, accent, warning and danger are moved in lightness until they pass the contrast rules (`theme/tests.rs` checks every hue in steps of 5, both chromas, tint on/off, both modes). 12 swatches plus a hue slider.
- **Preset colours tweaked to pass the same rules** (ADR 0002 values): Tide dark muted `8f9fa4`, Tide light accent `0b7f74`, Tide/Command light warning `b47a0e`, Paper light warning `a06f12` and muted `6e5f52`, Command dark muted `9090a5`, Command light danger `c23535`.
- **Fonts** (`fonts.rs`): Inter (UI) and Obel Mono (code) embedded and registered at startup; "Hide secrets in code" uses the Obel Mono Masked build (a family, because GPUI applies font features to every run below an element); "Use my font for headings too" drops the preset display font (Paper's Georgia).
- **Appearance overlay** (`features/appearance`): Ctrl+K "Theme…", "Accent colour…", "Fonts…". Colours tab (mode, which mode to edit, cards, swatches, slider, switches, contrast readout in the footer) and Fonts tab (two chips for now, switches, sample). Live preview; Esc / Cancel restores, Done / Enter saves.
- **Search field** fill is the border colour, or the pane colour with a border outline where the placeholder would not read (High contrast).
- **Scripts**: `drive.ps1 -Colors <preset | accent | theme> -Accent "hue,chroma,tint"`; `themes.ps1` (14 combinations × 3 presets × list, reader, toast, search, Appearance, after Esc); `sheet.ps1 -Match <regex>`.

Checked by screenshot (252 shots, `themes.ps1`): every theme in its mode and accents 268 / 150 muted / 25 untinted in both modes, in all three presets. Fixed: Appearance panel stayed after Esc, contrast readout below the fold, white slider fill, High contrast grey search field.

Checks: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass.

### Bench (bench.db, 2026-11-20, after 1G-U3, second run on the same exe, CPU ~7–14 %)

| Metric | Tide | Command | Paper | Budget |
|---|---|---|---|---|
| cold launch, worst of 3 | 317 ms (314–317) | 321 ms (311–321) | 321 ms (312–321) | 400 ms |
| j/k open to paint p95 | 14.2 ms | 10.0 ms | 10.5 ms | 50 ms |
| scroll dropped / blank | 0 / 1800, 0 | 0 / 1800, 0 | 0 / 1800, 0 | ≤ 2 %, 0 |

Rust bench: open + first page 2.6 ms, list page p95 0.20 ms, thread open p95 0.20 ms, search p95 12.8 ms, action write p95 1.07 ms (max 17.5 ms, over the 16 ms line; the check is on p95, as before). The pre-1G-U3 exe on the same day measured 328–373 ms worst cold launch, so the fonts cost nothing measurable; margin is ~80 ms.

## Next: Phase 1G-U3b (fonts)

1. Font picker: the other 10 interface and 9 code fonts from the plan (load on demand from disk or embed; measure cold launch either way). `fonts::UI_FONTS` / `CODE_FONTS` and the Fonts tab chips already take a list.
2. A use site for the code font in the reader (plain-text bodies, inline code); today only the Fonts tab sample uses it.
3. Then Phase 2 as before.

## Requests

### From the reader agent (1G-B): resolved

1. DirectComposition off: replaced by the vendored `topmost = false` patch (see 1G-B status).
2. `Reader::new(state, window, cx)`: accepted.
3. j/k budget: solved by swapping bodies in place, not by counting the title event.
