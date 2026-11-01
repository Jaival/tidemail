# Tidemail handoff

## Phase 0 status

- Rust workspace, SQLite schema/migrations and indexed read queries are in place.
- `fakegen` creates a reproducible 3-account mailbox with 50,000 messages and hostile HTML fixtures.
- Tauri 2 app shell, tauri-specta command/event registration, tray, hide-on-close, single-instance handling and first-paint reveal are wired.
- React/Vite skeleton provides dark/light system-following theme and a mailbox view backed by the local DB.
- The CLI accepts `--db <path>`; default is `%LOCALAPPDATA%\Tidemail\mail.db`.
- `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, frontend lint/typecheck/build pass. Rust bench against `bench.db`: open+first page 2.08 ms, list p95 0.13 ms, thread+body p95 0.14 ms; search/action are Phase 1 skips.

## Next

1. Complete frontend bindings export, build and smoke run on Windows.
2. Implement Phase 1 crate bodies and UI features against `bench.db`.
3. Add the WebView2 CDP E2E bench for cold launch, j/k paint, and virtual-list scrolling; Phase 0 currently has only the Rust benchmark runner.

## Open decisions

- Google OAuth client setup is required before account sync can be exercised.
- Phase 3 compose/editor and offline draft behavior remain intentionally undecided.
- Phase 4 projects/classification remain intentionally undesigned.
