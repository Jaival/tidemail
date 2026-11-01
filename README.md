# Tidemail

Tidemail is a local-first Gmail client for Windows 11. Mailbox reads use one local SQLite database; Gmail sync and actions are implemented in later phases.

## Development

- Install Rust (stable), Bun, and the WebView2 Runtime.
- Run `bun install` and `cargo check --workspace`.
- Start the desktop app with `bun run tauri dev`.
- Build with `bun run tauri build`.
- The app uses `%LOCALAPPDATA%\Tidemail\mail.db` by default. Pass `--db <path>` to the app or benchmark to select another database.
- Generate the deterministic benchmark mailbox with `cargo run -p tidemail-bench --bin fakegen -- --db bench.db`.
- Run the Rust database benchmark with `bun run bench -- --db bench.db`.

## Local data and Windows Defender

Tidemail stores its database and cached attachments under `%LOCALAPPDATA%\Tidemail`. If Windows Defender slows large local database writes, add that data directory as a Defender exclusion in Windows Security. No telemetry, auto-update, or code signing is used.

## Google sign-in setup

Phase 1 adds Google sign-in. It will use an OAuth client from your own Google Cloud project and load the client ID and secret from `%LOCALAPPDATA%\Tidemail\config.toml`; credentials are not compiled into the app. The refresh token will be kept in Windows Credential Manager.
