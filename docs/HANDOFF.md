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

## Next

1. Review the UI in the running app, then push `phase1` and open a PR to `main`.
2. Phase 2: get cold launch under 400 ms. Measure without the CDP port first, then look at WebView2 start (environment options, browser args) and the gap before the first `list_threads` call.

## Decisions (2026-11-02)

- Opening a thread marks it read (silent, no undo toast; `u` marks it unread again).
- `u` toggles read/unread.
- Spam and trash are synced (`includeSpamTrash=true`). No sidebar views for them (spec does not list them); search still includes them.

## Open decisions


- Google OAuth client setup is required before account sync can be exercised.
- Phase 3 compose/editor and offline draft behavior remain intentionally undecided.
- Phase 4 projects/classification remain intentionally undesigned.
