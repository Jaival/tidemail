# Handoff: renderer (phase1-renderer)

## Done

- `crates/render`
  - `sanitize(html, message_id)`: ammonia. Strips scripts, forms, frames, svg/math, templates, event handlers, `<title>`. Keeps `<style>` and `style`/`class`/`dir`/table-layout attributes, `center`, `font`. Every `<a>` gets `target="_blank" rel="noopener noreferrer"`.
  - URL schemes: `http https mailto tel cid data`. `img src` `cid:` (any case) becomes `{CID_ORIGIN}/<message_id>/<percent-encoded cid>`. `cid:` and `data:` on links are dropped. `file:`, `ftp:`, `javascript:` are dropped everywhere.
  - `SANITIZER_VERSION` = 2.
  - `safe_filename`, `attachment_path` (now sanitizes the name), `save_attachment` (writes file + `:Zone.Identifier` ADS, `ZoneId=3`), `save_attachment_in(base, account_id, message_id, filename, data) -> io::Result<PathBuf>`. `base` = `%LOCALAPPDATA%\Tidemail`.
  - `html_to_text` unchanged.
  - Tests use all hostile fixtures, a 1 MB newsletter, filenames and a real ADS write.
  - Timing: 1 MB sanitize = 168 ms release, 349 ms debug.
- `src/features/reader`: `Reader.tsx` (contract unchanged) and `srcdoc.ts`.
  - One iframe, always mounted, only `srcDoc` changes. Sandbox `allow-popups allow-popups-to-escape-sandbox`. CSP as in the spec; `https:` added to `img-src` when `body.loadImages`.
  - Thread = header + message strip (click to switch) + one iframe showing the selected message (default newest). Old content stays until the new one loads.
  - Iframe sizing: sandbox has no `allow-same-origin`, so the parent cannot measure content. The iframe fills the free pane height and scrolls inside. No measuring, no layout thrash.
  - Theme: HTML mail is drawn on a white page in both themes (newsletters assume light). Plain text and chrome follow the theme CSS variables.
  - "Load images" button (shown only if the HTML has remote images) calls `setSenderImages`, then refetches the body.
  - Attachment list (non-inline). Click calls `downloadAttachment`; result path or error is shown inline.
  - Body cache per message; cleared on `dataChanged` for the open thread.

## Requests for main thread

1. **JS opener** (reader attachments "open"): add `@tauri-apps/plugin-opener` to `package.json` and capability `opener:allow-open-path`. Then the Reader can call `openPath(path)` after download. Alternative: `Engine::download_attachment` returns the path and a new IPC `open_attachment(attachment_id)` opens it in Rust. Until then the Reader shows "Saved to <path>".
2. **Sync, `Engine::download_attachment`**: call `tidemail_render::save_attachment_in(&data_dir, account_id, message_id, &filename, &bytes)`, store the returned path in `attachments.local_path`, return it.
3. **Re-sanitize job (design only; sync owns it)**
   - Trigger: after startup and after each sync pass, `SELECT message_id FROM bodies WHERE html_sanitized IS NOT NULL AND sanitizer_version < ?` with `SANITIZER_VERSION`, in batches of 50, newest first, low priority.
   - For each: refetch `format=full` from Gmail (via batch), extract the raw HTML part, `sanitize(html, message_id)`, `UPDATE bodies SET html_sanitized=?, text=?, sanitizer_version=?`, then redo the FTS delete + insert for that message, then emit `data-changed` for the thread.
   - Offline: skip, retry next pass. Missing Gmail message: set the version, keep the old HTML (it was sanitized by an older, still-safe version) or clear it; decide in sync.
   - Rows sanitized by version 1 do not contain `<style>`; they are valid but stripped. Re-sanitize is cosmetic, not a security fix.

## Open questions

- Dark mode for HTML mail: white page now. Per-message inversion could come later.
- Search `snippetHtml` is not touched here.

## Known gaps (left out on purpose)

- `cid:` URLs inside CSS (`background:url(cid:...)`) are not rewritten; the fixture shows one. They do not load.
- A `data:` image bigger than the message is not size-capped.
- Inline `cid:` images with no matching part show a broken image (the handler returns 404).
- Iframe has no auto-height, so very long mails scroll inside the iframe, not the pane.
- `<style>` blocks from one message cannot leak (separate srcdoc), but a hostile `<style>` can restyle its own message (e.g. full-page overlay, see `css-overlay.html`). Accepted: it stays inside the iframe.
- Reply/forward/print buttons are not part of this phase.
