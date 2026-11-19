# Build prompt: Tidemail (native GPUI version)

You are building **Tidemail**, a local-first Gmail client for Windows 11. It is for my personal use only. Speed is the main feature: every click must read from a local database, never wait on Google.

UI decision: see `docs/adr/0001-native-ui-with-gpui.md`. Phase 0/1 were built on Tauri 2 + React (kept in git history at `a8cf15e`); Phase 1G replaces that UI with GPUI.

## Core idea

- A background sync engine copies Gmail into one local SQLite file.
- The UI only reads that file. Archive, label, read/unread and trash apply locally at once; an action queue replays them to Gmail with retries.
- The network only fills the database behind the user.

```
Gmail API  <->  Sync engine (Rust, tokio)  ->  SQLite + FTS5  <->  UI (GPUI, native)
                                                                    └─ reader pane: WebView2 child view (sanitized HTML only)
```

## Stack (fixed, do not change without asking)

- **Platform:** Windows 11 only. No macOS/Linux work.
- **Backend crates (unchanged):** Rust. `tokio`, `reqwest` (rustls), `rusqlite` with the `bundled` feature (includes FTS5), `serde`, `keyring` (Windows Credential Manager), `ammonia` (HTML sanitizing), `oauth2` crate for PKCE.
- **UI:** GPUI via `gpui-kit = "=0.9.0"` (facade over `gpui-component` 0.9.0 and `gpui-pre` 0.3.11; `vendor/gpui-pre-windows` patches startup, see the ADR). Use `gpui-component` widgets (input, list, theme, title bar) where they fit; plain GPUI elements otherwise. Pin exact versions; upgrade only on purpose and note it in `docs/HANDOFF.md`.
  - Lists: virtualized (`uniform_list` or the `gpui-component` list). Every list pages from the DB with keyset cursors, 100 rows per page.
  - Theme: dark + light from `gpui-component` theme, following `WindowAppearance`. Custom-drawn title bar so the whole window follows the theme.
- **Reader webview:** `gpui-webview = "=0.9.0"` (wry, WebView2). One instance, created after the first list frame. See "HTML mail rendering".
- **Tray:** `tray-icon` crate. **Single instance:** named mutex + named pipe (own code, small).
- **Open links/files:** GPUI `open_url` / `open_with_system` (or the `open` crate if those fall short).
- **Threading:** the UI calls the crates directly (no IPC). DB reads run on GPUI's background executor with a small pool of read connections; the sync engine runs on its own tokio runtime and sends "data changed" notices over a channel that the UI turns into entity updates. The UI thread never touches SQLite or the network.

## Speed budget (hard requirements, measured on a 50,000-message mailbox)

| Metric | Budget |
|---|---|
| Cold launch to filled inbox | < 400 ms (target 300 ms) |
| Open next/previous message (j / k) | < 50 ms |
| Search results per keystroke (p95) | < 30 ms |
| Network wait on archive / label / read | 0 ms |
| Scrolling the message list | 60 fps, no blank rows |

**Cold launch** = process creation (from the OS process start time) until the first list row is painted. The window opens DWM-cloaked and is uncloaked after the first frame with list rows (no blank shell). The database preload and sync engine start on a thread while GPUI starts.

`cargo build --release -p tidemail -p tidemail-bench`, then `targeteleaseench-all.exe --db bench.db`, runs both parts and fails if any budget is missed:

1. **Rust bench** (`bench` binary, release build): list query, thread open + body load, search query per keystroke (p95), action write (local only, 0 ms network).
2. **App bench:** the release app runs with `--bench <report.json>`. It measures cold launch, drives j/k (open to reader paint, including the webview content load), drives a scripted scroll (frame times, dropped frames, blank rows), writes the report and exits. Run 3 cold launches; report the worst. Every budget applies to every layout preset: `bench-all` runs the app bench once per preset (`--preset <name>`).

Both the app and `bench` take `--db <path>`. The fake mailbox lives in `bench.db`, so fake and real data can be benched side by side. Run `bench` at the end of every phase.

## Features

1. **Accounts:** multiple Gmail accounts, all in one SQLite file. Unified inbox plus per-account views. In the unified inbox each row shows a small color dot for its account.
2. **Layout presets** (see `docs/adr/0002-layout-presets.md`), chosen in the Ctrl+K palette, switched live:
   - **Tide** (default): three panes. Sidebar (accounts, inbox, starred, sent, drafts, labels with unread counts), virtualized thread list with two-line rows, reader.
   - **Command:** no sidebar. Tabs in the title bar, one-line rows over the full width, `Enter` opens the thread over the full width, `Esc` returns. `j/k` move the selection without opening. Key-hint bar at the bottom.
   - **Paper:** icon rail, three-line rows with avatars, wide reader set as a document.
   - Drafts are shown read-only from the `DRAFT` label until Phase 3.
3. **Reader:** safe HTML rendering (see below), plain-text fallback, attachments listed with download/open.
4. **Actions:** archive, trash, star, read/unread, add/remove label, move. All optimistic, all undoable (Ctrl+Z, 5 s toast). All list actions apply to the **whole thread** (`threads.modify` / `threads.trash`). "Move to X" = add X + remove `INBOX` in one `threads.modify`.
5. **Search:** FTS5 over sender, recipients, subject and body. Operators: `from:`, `to:`, `has:attachment`, `label:`, `is:unread`, `before:`, `after:`. Results update per keystroke. Results are **threads**, each showing the best-matching message snippet with highlights. Sort by date desc. Free text starts at 2 characters; operator-only queries run at once.
6. **Keyboard first:** `j/k` next/prev, `Enter` open, `e` archive, `#` trash, `s` star, `l` label, `u` mark unread, `/` search, `Ctrl+K` command palette, `g i` go to inbox. Show shortcuts in the palette.
7. **Theme:** dark + light for every preset (own palettes, not the `gpui-component` defaults). Follow the Windows setting; manual override in the Ctrl+K palette. The mail body is always on a light surface.
8. **Compose / reply / forward (Phase 3):** plain text + simple rich text, sent through `messages.send` (raw RFC 822). Drafts saved locally first. Detailed design (editor choice, Gmail drafts sync, offline send queue) is decided in a separate grilling session at the start of Phase 3.
9. **Later phase (Phase 4):** projects (user-defined groups of threads) and automatic classification. Not designed yet; grilled at the start of Phase 4.

**Tray:** the app keeps running in the system tray.
- Closing the window hides it (`on_window_should_close` returns false and hides the window with Win32 `ShowWindow`; GPUI has no hide on Windows). Sync keeps running while hidden.
- Tray icon (`tray-icon`): left-click shows the window; menu has **Show** and **Quit**. Quit stops sync cleanly and quits the GPUI app.
- Single instance: the first instance holds a named mutex and listens on a named pipe. A second launch sends "show" over the pipe and exits; the first instance shows, unminimizes and focuses its window.
- No autostart at Windows login (not requested).
- Cold-launch budget applies to process start. Showing the hidden window from the tray must paint at once (state stays in memory, no reload).

## Gmail sync engine (Rust)

- **Auth:** Google "installed app" OAuth with PKCE and a loopback redirect (`http://127.0.0.1:<random port>`). Scope `https://www.googleapis.com/auth/gmail.modify`. Store refresh tokens in Windows Credential Manager via `keyring`. `client_id` and `client_secret` come from my own Google Cloud project and are read from `%LOCALAPPDATA%\Tidemail\config.toml` (never compiled in). README notes: how to create the project and OAuth client; set the app to "In production" (unverified) so refresh tokens do not expire after 7 days.
- **Backfill:** newest mail first. `messages.list` pages, then `messages.get` in batch requests (`/batch/gmail/v1`, max 50 calls per batch) with `format=metadata` for the list, bodies fetched in a second pass (`format=full`), newest first, so the inbox fills in seconds.
  - Body policy: fetch all bodies, newest first. If an account has more than ~300k messages, fetch bodies for the last 2 years only; older bodies are fetched when opened.
  - The body pass also fetches inline `cid:` parts under 200 KB, so j/k stays fast on receipts and signatures.
- **Live changes:** poll `history.list` from the stored `historyId` every 5–10 s while the app runs (window visible or hidden in the tray). If Google returns 404 (history expired), run a partial resync = **full id diff**: list all message ids, fetch new ones, delete removed ones, refetch labels for changed ones.
- **Action queue:** table `action_queue(id, account_id, kind, payload_json, attempts, next_attempt_at, last_error)`. Exponential backoff, max 8 attempts.
  - **Undo:** every new action waits 5 s before its first send (`next_attempt_at = now + 5s`). Undo inside 5 s deletes the row (nothing reaches Gmail). Undo later enqueues the inverse action.
  - **Reconciliation:** when `history.list` brings a change for a thread with pending actions, apply the remote change, then re-apply the pending actions on top. Local state is the truth until Gmail confirms.
  - **Failure:** after 8 attempts mark failed; the next sync restores the Gmail state for that thread; the status area shows "1 action failed" with Retry / Dismiss.
- **Quota:** respect 250 quota units/user/second; handle 429 and 403 `rateLimitExceeded` with backoff.
- **Prefetch:** when a message is opened, prefetch bodies of the next 10 messages in the list.

## Database

- One file, default `%LOCALAPPDATA%\Tidemail\mail.db` (override with `--db`). WAL mode, `synchronous=NORMAL`, `mmap_size` 256 MB, prepared statements cached.
- Tables (adjust as needed, keep it lean):
  - `accounts`, `labels`, `attachments`, `action_queue`, `sync_state(account_id, history_id, backfill_cursor)`.
  - `messages`: ids, thread_id, account_id, from_name, from_email, to_text (to + cc names/emails), subject, snippet, internal_date, is_unread, is_starred, has_attachment, body_state. **No bodies, no label list column.**
  - `message_labels(message_id, label_id)`.
  - `bodies(message_id PK, html_sanitized, text, sanitizer_version)`: kept out of `messages` so list paging stays small.
  - `threads` (denormalized, recomputed by Rust on every message change): id, account_id, last_date, subject, snippet, participants, message_count, unread_count, has_attachment, is_starred.
  - `thread_labels(label_key, last_date DESC, thread_id, account_id)`: denormalized; a thread has a label if any of its messages has it. System labels (`INBOX`, `STARRED`, `SENT`, …) use the same `label_key` across accounts, so the unified inbox is one index range scan.
  - `sender_prefs(email, load_images)`.
- `messages_fts` FTS5 table, **external content over a view `fts_src`** (join of `messages` + `bodies`), columns `from_name, from_email, to_text, subject, body_text`. Tokenizer `unicode61 remove_diacritics 2`, `prefix='2 3'`. Views cannot have triggers, so Rust maintains FTS rows explicitly: insert on metadata arrival (empty body), delete + re-insert when the body arrives. `snippet()` / `highlight()` must work.
- Indexes for each list query. Check every list query with `EXPLAIN QUERY PLAN`; no full table scans.
- Schema migrations with `PRAGMA user_version`.

## HTML mail rendering (security matters)

- Sanitize every HTML body with `ammonia` **at ingest** in Rust and store only the sanitized HTML (`bodies.html_sanitized`) with a `sanitizer_version`. A background job re-sanitizes old rows after a sanitizer change (raw HTML is refetched from Gmail). Nothing is sanitized at read time.
- Remove scripts, forms, event handlers, `javascript:` URLs. **Allow `<style>` elements and `style` attributes** (newsletters need them); the CSP is the safety net for CSS.
- GPUI does not render HTML. The reader body is one WebView2 child view (`gpui-webview`) placed in the reader pane. Header, attachments list and "Load images" button are native GPUI; only the body is in the webview.
  - **Scripts off** in the webview: wry `WebViewBuilder::with_javascript_disabled()` (sets WebView2 `IsScriptEnabled = false`). Also disable dev tools, context menu, status bar, zoom shortcuts, and default drag-and-drop.
  - Created once, after the first list frame (off the cold-launch path), and kept warm: open a message by loading new content into the same webview; never create one per message.
  - The OS draws the webview above GPUI. While an overlay (Ctrl+K palette, label/move picker, toasts over the reader) is open, hide the webview. Keyboard focus stays in GPUI: after a click in the webview, call wry `focus_parent()`, so j/k and all shortcuts keep working.
- CSP (meta tag in the document, and as a response header when served from a custom scheme): `default-src 'none'; style-src 'unsafe-inline'; img-src data: http://tidemail-cid.localhost; font-src data:`. When the sender is allowed to load images, `img-src` adds `https:`. Show a "Load images" button per sender; remember the choice in `sender_prefs`.
- Inline `cid:` images: ammonia rewrites `cid:` URLs to the custom URI scheme `tidemail-cid`, served by Rust from `inline_blobs` (wry custom protocol). On Windows, WebView2 exposes it as `http://tidemail-cid.localhost/<message>/<percent-encoded cid>` (`tidemail_render::CID_ORIGIN`). They show even while remote images are blocked.
- Links: ammonia sets `target="_blank" rel="noopener noreferrer"` on every link. The wry navigation handler and new-window handler cancel every request that is not the reader document itself, and open `http`, `https` and `mailto` URLs in the default app. Never navigate inside the app.
- Attachments: downloaded on demand to `%LOCALAPPDATA%\Tidemail\attachments\<account>\<message>\<name>`; no eviction for now. Write a `Zone.Identifier` stream (Mark-of-the-Web) on every saved file so Windows/SmartScreen/Office warn on risky files. Open with the default app.

## Build order (foundation first, then parallel)

**Phase 0 and Phase 1 – done (Tauri + React, 2026-11-02).** Backend crates, schema, fake mailbox, sync engine, Gmail client, search, renderer are done and stay. See `docs/HANDOFF.md` and `docs/handoff/*.md`.

**Phase 1G-A – Native foundation (main thread, Opus):**
- Remove `src-tauri`, `src/`, `e2e/`, the JS tooling and the `specta` derives in `tidemail-db`.
- New app crate `crates/app` (binary `tidemail`): window, custom title bar, theme (follows Windows, manual override), tray, hide-on-close, single instance, `--db`, sync engine start, first-frame reveal.
- Sidebar (accounts, inbox, starred, sent, drafts, labels) and the virtualized thread list with account dots, paging through `tidemail-db`.
- App bench mode (`--bench`) for cold launch and scroll.
- **Freeze** for 1G-B: the module layout in `crates/app/src/features/<area>/`, the shared app state (selection, current view, action dispatch, overlay stack), and the data-changed channel.

**Phase 1G-U1 – Presets foundation (main thread, Opus), before 1G-B:** `preset.rs`, palettes, shared `ui.rs` primitives, navigation model with unread counts and its three renderers, three row renderers, shell layout per preset, selection/reading split, bench per preset. Steps in `docs/HANDOFF.md`.

**Phase 1G-B – Three parallel agents** (Sonnet delegates, each in its own git worktree, against the fake mailbox). Each agent reads `PresetSpec` and the palette; none hard-codes a layout or a colour:
1. **Reader** (`features/reader`): webview host, scripts off, warm reuse, CSP, `tidemail-cid` protocol, link interception, image blocking + sender prefs, attachments open/save, hide-under-overlay, plain-text fallback, j/k in the bench.
2. **Search** (`features/search`): input, per-keystroke results via `tidemail-search`, highlighted snippets, `/` shortcut slot.
3. **Commands** (`features/palette`, `features/status`): Ctrl+K palette with shortcuts listed, all keyboard shortcuts, label/move pickers, undo toasts (Ctrl+Z, 5 s), status area, prefetch setting.

Rules for Phase 1G-B:
- Each agent edits only its own folder.
- Changes to shared app state, schema or crate APIs are written as requests in `docs/HANDOFF.md`; the main thread (Opus) applies them.
- The main thread reviews and merges in this order: reader, search, commands.

**Phase 1G-U2 – Visual pass (main thread, Opus), after the 1G-B merge:** reader chrome, palette and pickers, empty/loading/error states, checked by screenshot in 3 presets × 2 modes, then the full bench.

**Phase 2 – Connect:** replace the fake mailbox with real accounts, run `bench` against both `bench.db` and the real DB, fix every missed budget.

**Phase 3 – Compose, reply, drafts.** Starts with its own grilling session.

**Phase 4 – Projects and classification.** Starts with its own grilling session.

At the end of each phase: `cargo build --release`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `bench`. Update `docs/HANDOFF.md`.

## Rules

- No telemetry, no auto-update, no code signing (personal use; SmartScreen warning is fine).
- Windows Defender: add a README step to exclude the data folder (it slows SQLite writes).
- Do not add features I did not list. Ask before adding code for unlikely edge cases.
- Errors: show sync errors in a small status area ("Synced 12 s ago" / "Offline" / "3 actions pending" / "1 action failed"). Never block the UI on errors.

Start with Phase 1G-A. Before writing code, show me the module layout for approval.
