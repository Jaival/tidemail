# Tidemail

Tidemail is a local-first Gmail client for Windows 11. Mailbox reads use one local SQLite database; Gmail sync and actions are implemented in later phases.

## Development

- Install Rust (stable), Bun, and the WebView2 Runtime.
- Run `bun install` and `cargo check --workspace`.
- Start the desktop app with `bun run tauri dev`.
- Build with `bun run tauri build`.
- The app uses `%LOCALAPPDATA%\Tidemail\mail.db` by default. Pass `--db <path>` to the app or benchmark to select another database.
- Generate the deterministic benchmark mailbox with `cargo run -p tidemail-bench --bin fakegen -- --db bench.db`.
- Run the benchmarks with `bun run bench -- --db bench.db`. It runs the Rust bench, builds the release app (`tauri build --no-bundle`) and runs the E2E bench (`e2e/bench.ts`, Playwright over WebView2 CDP, under Node). Close Tidemail first: single-instance would hand the launch to the running app. Add `--skip-e2e` for the Rust part only.

## Local data and Windows Defender

Tidemail stores its database and cached attachments under `%LOCALAPPDATA%\Tidemail`. No telemetry, auto-update, or code signing is used.

Exclude that folder from Windows Defender, because real-time scanning slows SQLite writes:

1. Open Windows Security, then Virus & threat protection, then Manage settings.
2. Under Exclusions choose Add or remove exclusions, then Add an exclusion, then Folder.
3. Pick `%LOCALAPPDATA%\Tidemail` (usually `C:\Users\<you>\AppData\Local\Tidemail`).

## Google sign-in setup

Tidemail signs in with an OAuth client from your own Google Cloud project. The client ID and secret are read from a file and are not compiled into the app. Refresh tokens are kept in Windows Credential Manager.

1. Open the [Google Cloud console](https://console.cloud.google.com/) and create a project (for example `tidemail`).
2. Go to APIs & Services, then Library. Search for **Gmail API** and click Enable.
3. Go to APIs & Services, then OAuth consent screen (Google Auth platform). Choose user type **External**. Fill in the app name and your email. Add the scope `https://www.googleapis.com/auth/gmail.modify`.
4. On the same screen set the publishing status to **In production** (Publish app). The app stays unverified, so sign-in shows a "Google hasn't verified this app" warning: choose Advanced, then continue. Do not leave it in **Testing**: refresh tokens of apps in Testing expire after 7 days.
5. Go to APIs & Services, then Credentials. Choose Create credentials, then OAuth client ID, with application type **Desktop app**. Copy the client ID and client secret.
6. Create `%LOCALAPPDATA%\Tidemail\config.toml`:

   ```toml
   client_id = "1234567890-abc.apps.googleusercontent.com"
   client_secret = "GOCSPX-..."
   ```

If the file is missing or invalid, sign-in fails with an error that names its path.
