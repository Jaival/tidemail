/** Origin of the `tidemail-cid` scheme on WebView2 (`tidemail_render::CID_ORIGIN`). */
const CID_ORIGIN = "http://tidemail-cid.localhost";

/**
 * Document for the reader iframe. The CSP is exactly the one in the spec; `https:` is
 * added to `img-src` only when the sender may load remote images. `html` is already
 * sanitized at ingest and is never sanitized here.
 *
 * HTML mail is drawn on a white page in both themes: newsletters set dark text and
 * light backgrounds, and inverting them is guesswork.
 */
export function buildSrcdoc(html: string, loadImages: boolean): string {
  const img = `data: ${CID_ORIGIN}${loadImages ? " https:" : ""}`;
  const csp = `default-src 'none'; style-src 'unsafe-inline'; img-src ${img}; font-src data:`;
  return (
    `<!doctype html><html><head><meta charset="utf-8">` +
    `<meta http-equiv="Content-Security-Policy" content="${csp}">` +
    `<style>html{color-scheme:light}body{margin:0;padding:16px;background:#fff;color:#1b1d24;` +
    `font:14px/1.5 "Segoe UI",sans-serif;overflow-wrap:anywhere}img{max-width:100%;height:auto}</style>` +
    `</head><body>${html}</body></html>`
  );
}

/** True when the sanitized HTML points at remote (http/https) images. */
export function hasRemoteImages(html: string): boolean {
  return /<img\b[^>]*\bsrc\s*=\s*["']?https?:/i.test(html);
}
