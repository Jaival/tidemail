# Tidemail

Tidemail is a local-first Gmail client for Windows 11. Mailbox reads use one local SQLite database; a background engine syncs Gmail. The UI is native Rust (GPUI).

## Development

- Install Rust (stable, MSVC) and the WebView2 Runtime (used by the reader pane only). The GPUI build needs `fxc.exe` from the Windows SDK; set `GPUI_FXC_PATH` if it is not found.
- Build and run: `cargo run --release -p tidemail`. The first build takes several minutes (GPUI).
- The app uses `%LOCALAPPDATA%\Tidemail\mail.db` by default. Pass `--db <path>` to the app or benchmark to select another database. UI settings (theme, prefetch count) live in `settings.json` next to the database.
- Generate the deterministic benchmark mailbox with `cargo run --release -p tidemail-bench --bin fakegen -- --db bench.db`.
- Run the benchmarks: `cargo build --release -p tidemail -p tidemail-bench`, then `targeteleaseench-all.exe --db bench.db`. It runs the Rust bench, then the release app 3 times with `--bench` on a copy of the database (cold launch, j/k open to paint, scrolling). Add `--skip-app` for the Rust part only.
- `vendor/gpui-pre-windows` is `gpui-pre-windows` 0.3.11 with one change (`vendor/gpui-pre-windows-parallel-dx.patch`): DirectX devices are created on a thread while DirectWrite starts. Re-apply it when upgrading GPUI.

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
