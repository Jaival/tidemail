# Build prompt: Tidemail (Tauri 2 version)

You are building **Tidemail**, a local-first Gmail client for Windows 11. It is for my personal use only. Speed is the main feature: every click must read from a local database, never wait on Google.

## Core idea

- A background sync engine copies Gmail into one local SQLite file.
- The UI only reads that file. Archive, label, read/unread and trash apply locally at once; an action queue replays them to Gmail with retries.
- The network only fills the database behind the user.

```
Gmail API  <->  Sync engine (Rust, background)  ->  SQLite + FTS5  <->  UI (WebView2)
```

## Stack (fixed, do not change without asking)

- **Shell:** Tauri 2, Windows only (WebView2). No macOS/Linux work.
- **Backend:** Rust. `tokio`, `reqwest` (rustls), `rusqlite` with the `bundled` feature (includes FTS5), `serde`, `keyring` (Windows Credential Manager), `ammonia` (HTML sanitizing), `oauth2` crate for PKCE, `tauri-plugin-opener` (open links/files in default apps).
- **Frontend:** React + TypeScript + Vite, Tailwind, TanStack Virtual for every list. No UI framework that brings heavy runtime cost.
- **JS tooling:** use **Bun** as package manager and script runner (`bun install`, `bun run …`, `bunx`). Tauri's `beforeDevCommand` / `beforeBuildCommand` use `bun run`. If a tool does not work under Bun's runtime (e.g. the Playwright test runner), run it with Node but still install it with Bun.
- **Types:** `tauri-specta` generates TS types **and** typed wrappers for every Tauri command and event from Rust. No hand-written IPC types.
  - No stable tauri-specta 2.x exists. Pin this exact compatible triple: `tauri-specta = "=2.0.0-rc.26"` (features `derive`, `typescript`), `specta = "=2.0.0-rc.26"`, `specta-typescript = "=0.0.13"`. Do not mix in `specta` rc.27 or `specta-typescript` 0.0.14. Upgrade only on purpose.
  - Commit the generated `src/bindings.ts` and review its diff on every upgrade (RCs change the output).
  - `i64`/`u64` fail export by default. Call `Builder::dangerously_cast_bigints_to_number()` (safe: timestamps are ms, far below 2^53). Gmail ids stay `String`.
  - Stay on Tauri 2.x (Tauri 3 is alpha). See `docs/research/tauri-specta-and-tray.md`.
- **IPC:** Tauri commands for reads (paged, max ~100 rows per call), Tauri events for "data changed" pushes from Rust. Never send the whole mailbox over IPC.

## Speed budget (hard requirements, measured on a 50,000-message mailbox)

| Metric | Budget |
|---|---|
| Cold launch to filled inbox | < 400 ms (target 300 ms) |
| Open next/previous message (j / k) | < 50 ms |
| Search results per keystroke (p95) | < 30 ms |
| Network wait on archive / label / read | 0 ms |
| Scrolling the message list | 60 fps, no blank rows |

**Cold launch** = process start until the first list row is painted. The frontend sends a performance mark to Rust at first paint. The window stays hidden until then (no blank shell).

The `bench` command has two parts; one `bun run bench` script runs both and fails if any budget is missed:

1. **Rust bench** (`bench` binary, release build): list query, thread open + body load, search query per keystroke (p95), action write (local only, 0 ms network).
2. **E2E bench:** Playwright connects to WebView2 over CDP (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>`) and measures cold launch, j/k open-to-paint, and scroll (dropped frames, blank rows).

Both the app and `bench` take `--db <path>`. The fake mailbox lives in `bench.db`, so fake and real data can be benched side by side. Run `bench` at the end of every phase.

## Features

1. **Accounts:** multiple Gmail accounts, all in one SQLite file. Unified inbox plus per-account views. In the unified inbox each row shows a small color dot for its account.
2. **Three panes:** sidebar (accounts, inbox, starred, sent, drafts, labels), virtualized message list (threads grouped), reader. Drafts are shown read-only from the `DRAFT` label until Phase 3.
3. **Reader:** safe HTML rendering (see below), plain-text fallback, attachments listed with download/open.
4. **Actions:** archive, trash, star, read/unread, add/remove label, move. All optimistic, all undoable (Ctrl+Z, 5 s toast). All list actions apply to the **whole thread** (`threads.modify` / `threads.trash`). "Move to X" = add X + remove `INBOX` in one `threads.modify`.
5. **Search:** FTS5 over sender, recipients, subject and body. Operators: `from:`, `to:`, `has:attachment`, `label:`, `is:unread`, `before:`, `after:`. Results update per keystroke. Results are **threads**, each showing the best-matching message snippet with highlights. Sort by date desc. Free text starts at 2 characters; operator-only queries run at once.
6. **Keyboard first:** `j/k` next/prev, `Enter` open, `e` archive, `#` trash, `s` star, `l` label, `u` mark unread, `/` search, `Ctrl+K` command palette, `g i` go to inbox. Show shortcuts in the palette.
7. **Theme:** dark + light. Follow the Windows setting; manual override in the Ctrl+K palette.
8. **Compose / reply / forward (Phase 3):** plain text + simple rich text, sent through `messages.send` (raw RFC 822). Drafts saved locally first. Detailed design (editor choice, Gmail drafts sync, offline send queue) is decided in a separate grilling session at the start of Phase 3.
9. **Later phase (Phase 4):** projects (user-defined groups of threads) and automatic classification. Not designed yet; grilled at the start of Phase 4.

**Tray:** the app keeps running in the system tray.
- Closing the window hides it (`WindowEvent::CloseRequested` → `prevent_close()` + `hide()`). Sync keeps running while hidden.
- Tray icon (`tauri` feature `tray-icon`, `TrayIconBuilder`): left-click shows the window; menu has **Show** and **Quit**. Quit calls `app.exit(0)`.
- If `RunEvent::ExitRequested` is handled, call `prevent_exit()` only when `code.is_none()`, otherwise tray Quit is blocked.
- `tauri-plugin-single-instance` (registered first): a second launch shows, unminimizes and focuses the existing window.
- No autostart at Windows login (not requested).
- Cold-launch budget applies to process start. Showing the hidden window from the tray must paint at once (no reload).

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
- Render inside a sandboxed `<iframe srcdoc>` with **no `allow-scripts`**, sandbox `allow-popups allow-popups-to-escape-sandbox` only.
- CSP (meta tag in srcdoc): `default-src 'none'; style-src 'unsafe-inline'; img-src data: tidemail-cid:; font-src data:`. When the sender is allowed to load images, `img-src` adds `https:`. Show a "Load images" button per sender; remember the choice in `sender_prefs`.
- Inline `cid:` images: ammonia rewrites `cid:` URLs to a custom URI scheme `tidemail-cid://<message>/<cid>`, served by Rust from the local attachment cache. They show even while remote images are blocked.
- Links: ammonia sets `target="_blank" rel="noopener noreferrer"` on every link. Rust intercepts new-window and navigation requests from the webview, cancels them, and opens the URL in the default browser via `tauri-plugin-opener`. Never navigate inside the app. (Confirm the exact Tauri 2 hook in Phase 0.)
- Keep one iframe warm and swap its content; do not create a new one per message.
- Attachments: downloaded on demand to `%LOCALAPPDATA%\Tidemail\attachments\<account>\<message>\<name>`; no eviction for now. Write a `Zone.Identifier` stream (Mark-of-the-Web) on every saved file so Windows/SmartScreen/Office warn on risky files. Open via `tauri-plugin-opener`.

## Build order (foundation first, then parallel)

**Phase 0 – Foundation (one agent, do this first):**
- `git init`. Tauri app skeleton (Bun), Rust workspace, frontend skeleton, theme (dark + light, follows Windows). Tray icon, hide-on-close and single-instance live in the `src-tauri` app crate and are done here.
- **Freeze** for Phase 1: the crate layout (`crates/db`, `crates/gmail`, `crates/sync`, `crates/search`, `crates/render`, `src-tauri` app crate) and the TS folders (`src/features/<area>/`); the schema + migrations; the IPC command list (via `tauri-specta`).
- Implement the basic reads as **real queries** on the fake DB (not stubs): list threads by label (paged), get thread, get body.
- A **fake mailbox generator** with a fixed seed: 3 accounts, 50,000 messages, ~8,000 threads, realistic senders, subjects, HTML bodies (newsletters, receipts, plain replies), labels, attachments metadata. Include hostile/tricky HTML: scripts, `onerror`, `javascript:` links, tracking pixels, `cid:` images, very wide tables, 1 MB newsletters. Writes `bench.db`.
- The `bench` command (both parts).
- Write `docs/HANDOFF.md`: what is done, what is next, open decisions.

**Phase 1 – Five parallel agents** (Sonnet delegates, each in its own git worktree, each works only against the fake mailbox, so nobody waits for Google sign-in):
1. **Google sign-in** (`crates/gmail`): OAuth PKCE, keyring storage, config.toml, API client, batch helper.
2. **Sync engine** (`crates/sync`): backfill, history polling, full id diff, action queue (undo delay, reconciliation, failure), prefetch.
3. **UI shell** (`src/features/shell`, list, reader layout): sidebar, virtualized list, account dots, Ctrl+K palette, all keyboard shortcuts, undo toasts, status area. Gives search a slot and the `/` shortcut.
4. **Mail renderer** (`crates/render`, `src/features/reader`): sanitizing, CSP, sandboxed iframe, warm iframe reuse, image blocking + sender prefs, `tidemail-cid://` scheme, link interception, attachments + Mark-of-the-Web.
5. **Search** (`crates/search`, `src/features/search`): FTS5 maintenance + queries, operator parser, input + results list, per-keystroke results, highlight matches.

Rules for Phase 1:
- Each agent edits only its own crate / folder.
- Schema or IPC command changes are written as requests in `docs/HANDOFF.md`; the main thread (Opus) applies them.
- The main thread reviews and merges in this order: renderer, search, UI shell, sign-in, sync (sync last; it touches the most shared state).

**Phase 2 – Connect:** replace the fake mailbox with real accounts, run `bench` against both `bench.db` and the real DB, fix every missed budget.

**Phase 3 – Compose, reply, drafts.** Starts with its own grilling session.

**Phase 4 – Projects and classification.** Starts with its own grilling session.

At the end of each phase: build, `cargo clippy -- -D warnings`, `cargo test`, frontend lint + typecheck (`bun run lint`, `bun run typecheck`), `bench`. Update `docs/HANDOFF.md`.

## Rules

- No telemetry, no auto-update, no code signing (personal use; SmartScreen warning is fine).
- Windows Defender: add a README step to exclude the data folder (it slows SQLite writes).
- Do not add features I did not list. Ask before adding code for unlikely edge cases.
- Errors: show sync errors in a small status area ("Synced 12 s ago" / "Offline" / "3 actions pending" / "1 action failed"). Never block the UI on errors.

Start with Phase 0. Before writing code, show me the module layout and the schema for approval.
