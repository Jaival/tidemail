# Handoff: reader (phase1g-b-reader)

## Done

- `features/reader/{mod,host,page}.rs`.
  - `host.rs`: one WebView2 (`gpui-webview` 0.9.0 + `lb-wry`), created after frame 2 (`Reader::new` chains two `on_next_frame`), then kept warm. Scripts off, no devtools, no context menu, no accelerator keys, no zoom, drag-drop blocked.
  - Navigation handler allows only the `NavigateToString` document (`data:` / `about:blank`); `http`, `https`, `mailto` open through `tidemail_gmail::open_in_browser`; all else is cancelled. New-window handler: same, always denied.
  - `tidemail-cid` asynchronous custom protocol: `tidemail_render::load_cid` on the background executor.
  - Page load: the document `<title>` carries a nonce; the title event and the page-load `Finished` event drive `Reader::shown()` (late events of replaced documents are ignored).
  - `GotFocus` handler on the WebView2 controller calls `SetFocus(parent)`, so j/k and shortcuts keep working after a click in the page.
  - `page.rs`: CSP (`https:` in `img-src` only when the sender allows images), document wrapper, remote-image detection, cid path parsing, scheme check. Unit tests.
  - `mod.rs`: native header (subject in `spec.display_font`), label chips, toolbar (archive, trash, star, label, move; dispatch the existing actions), strip of collapsed messages (click expands, last message open by default), sender line, "Load images from this sender" (`write::set_sender_images`, then body reload; shown only when the HTML has remote images), non-inline attachments (`engine.download_attachment`, then `cx.open_with_system`), plain-text fallback (native text) when there is no HTML, `load_html` fails or WebView2 is missing. Body column max 760 px, centred, on `colors.paper`.
  - Webview hidden while `overlay_open()`, with no thread, or in Command while not `reading` (events `ReadingChanged`, `OverlaysChanged`, `SettingsChanged`).
  - A missing body asks the engine to fetch it (`prefetch_bodies`).
- `Reader::new` takes `window` now (needed to schedule the host); the one call in `shell/mod.rs` is updated.
- `main.rs`: sets `GPUI_DISABLE_DIRECT_COMPOSITION=1` before GPUI starts. See Requests.
- `ui.rs`: removed the two `#[allow(dead_code)]`.
- App `Cargo.toml`: `wry` (= `lb-wry` 0.53.3), `webview2-com` 0.38 (both already in the lock), windows-sys feature `Win32_UI_Input_KeyboardAndMouse`.

## Checks (2026-11-19)

Clippy `-D warnings`, build and `cargo test --workspace` pass. Checked by screenshot (Tide, Command, Paper dark): the webview shows the receipt fixture with its cid image, scroll bar and the 760 px paper column.

Bench smoke (debug build, bench.db copy, 100 j/k steps, no step hit the wait cap):

| Preset | jk p95 | jk max |
|---|---|---|
| tide | 157 ms | 183 ms |
| command | 99 ms | 108 ms |

Debug numbers; `shown()` now waits for the real page load. Run the release `bench-all` for the 50 ms budget.

## Requests

See the end of `docs/HANDOFF.md`.

## Known gaps

- `NavigateToString` takes at most 2 MB of document; larger bodies fail and show as plain text.
- Keyboard scrolling inside the page does not work (focus returns to GPUI at once); mouse wheel and links do.
- Other messages sit in the strip above the open one, whatever their order in the thread.
- Label and Move buttons dispatch `PickLabel` / `PickMove`; they do nothing until the commands agent lands the pickers.
- WebView2 user data folder is the wry default (next to the exe), not `%LOCALAPPDATA%\Tidemail`.
- Not checked by hand: attachment download/open (no real Gmail), the "Load images" click, link open, Paper light.

## Changed by the main thread after the merge (2026-11-20)

See `docs/HANDOFF.md`, "Phase 1G-B status". In short: the host is built in a spawned task outside any `App` borrow (`host::build` no longer takes `window`/`cx`; `Reader::attach_host` wraps it); the body area is a `canvas` that sets WebView2 bounds only on change; `GPUI_DISABLE_DIRECT_COMPOSITION` is gone (vendored GPUI patch instead); bodies with an unchanged image policy are swapped in with `page::swap_script` instead of a navigation. The debug j/k numbers and the DirectComposition request above are out of date. Release j/k p95 is 6.7–8.4 ms.
