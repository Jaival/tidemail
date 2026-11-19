# 0001: Native UI with GPUI instead of Tauri + React

Date: 2026-11-06. Status: accepted.

## Context

Phase 0 and Phase 1 shipped on Tauri 2 (WebView2) with a React + TypeScript frontend. Every Rust budget passes with a wide margin (list page p95 0.16 ms, thread open 1.08 ms, search p95 11.3 ms). The one budget that fails is cold launch: 538 ms worst of 3 against 400 ms. About 390–460 ms of that is WebView2 environment start before our code runs, so it cannot be fixed in our own code.

## Decision

Replace the Tauri shell and the React frontend with a native Rust UI on GPUI (`gpui-component` 0.9.0, which uses `gpui-pre` 0.3.11). Pin exact versions and upgrade only on purpose.

- Keep all library crates (`db`, `gmail`, `sync`, `search`, `render`, `bench`), the schema, and the sync, action queue and search designs unchanged.
- GPUI cannot render HTML. The reader pane hosts one WebView2 child view (`gpui-webview` 0.9.0, wry) with scripts disabled. It is created after the first list frame, so WebView2 is off the cold-launch path. Sanitizing at ingest, CSP, the `tidemail-cid` scheme and link interception stay.
- The OS draws the webview above GPUI. Overlays (Ctrl+K palette, pickers, toasts) hide the webview while they are open.
- Custom-drawn title bar, so dark/light covers the whole window (GPUI cannot set the native title bar theme on Windows).
- Tray via the `tray-icon` crate; single-instance via a named mutex plus a named pipe that tells the first instance to show.
- The UI calls the crates directly. No IPC layer, no `tauri-specta`, no TypeScript, no Bun.
- The E2E bench (Playwright over CDP) becomes a `--bench` mode in the app. It drives j/k and scroll, records frame times, and measures cold launch from process creation.
- Accessibility: GPUI has weak screen-reader support. Accepted for a personal app.

## Startup findings (Phase 1G-A, 2026-11-06)

- Stock GPUI on Windows needs 450–630 ms to the first frame on this machine: DirectWrite start ~130–150 ms and DirectX device creation ~150–185 ms run one after the other, before any window exists.
- Fix: `vendor/gpui-pre-windows` (via `[patch.crates-io]`) creates the DirectX devices on a thread while DirectWrite starts. Diff: `vendor/gpui-pre-windows-parallel-dx.patch` (~15 lines). Re-apply on every GPUI upgrade, or drop it if upstream does the same.
- GPUI draws no frames for a hidden window. The window opens hidden, is shown DWM-cloaked (`DWMWA_CLOAK`), and is uncloaked after its first frame. GPUI has no window hide on Windows, so hide/show use Win32 `ShowWindow`.
- Result: cold launch 256–297 ms (process creation to first list frame), against 433–538 ms on Tauri.

## Consequences

- About 1.7k lines of TypeScript and the `src-tauri` crate are removed. The UI shell, list, reader host, search UI and palette are rewritten in Rust (Phase 1G).
- `tidemail-db` drops its `specta` derives.
- Build times grow (GPUI is large). Release build stays `lto = "thin"`.
- GPUI APIs change between releases. Pinning and a small set of used APIs limit the cost.
- Tauri decisions that no longer apply: the pinned specta triple, `bindings.ts`, Tauri hooks for navigation/new window, `tauri-plugin-*`, the "max ~100 rows per IPC call" rule (the list still pages 100 rows at a time from the DB).
- Phase 0/1 Tauri code stays in git history on `main` (commit `a8cf15e`) for reference.
