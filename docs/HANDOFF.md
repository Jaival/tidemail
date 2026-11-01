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

## Next

1. Implement Phase 1 crate bodies and UI features against `bench.db`.
2. Phase 2: get cold launch under 400 ms. Measure without the CDP port first, then look at WebView2 start (environment options, browser args) and the gap before the first `list_threads` call.

## Open decisions

- Google OAuth client setup is required before account sync can be exercised.
- Phase 3 compose/editor and offline draft behavior remain intentionally undecided.
- Phase 4 projects/classification remain intentionally undesigned.
