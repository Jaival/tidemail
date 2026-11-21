//! The HTML document the webview shows: CSP, page colours, sanitized body.
//! Pure functions, no GPUI.

use gpui_kit::{Hsla, Rgba};
use tidemail_render::CID_ORIGIN;

/// `#rrggbb` of a colour.
pub fn css_color(c: Hsla) -> String {
    let c: Rgba = c.into();
    let byte = |v: f32| (v * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

/// Content-Security-Policy of the reader document (docs/SPEC.md).
/// `https:` images only when the sender is allowed to load them.
pub fn csp(load_images: bool) -> String {
    let remote = if load_images { " https:" } else { "" };
    format!(
        "default-src 'none'; style-src 'unsafe-inline'; img-src data: {CID_ORIGIN}{remote}; font-src data: {CID_ORIGIN}"
    )
}

/// Path prefix under [`CID_ORIGIN`] that serves a bundled font by family name.
pub const FONT_PATH: &str = "/font/";

/// Full document for `html` (sanitized at ingest). `nonce` goes into `<title>`:
/// the page-load handler compares it, so a late event of an older document is ignored.
/// `code_family` (a bundled font, served from [`FONT_PATH`]) draws `code`,
/// `pre`, `kbd`, `samp` and `tt` where the mail sets no font of its own.
pub fn document(
    html: &str,
    nonce: u64,
    load_images: bool,
    paper: &str,
    ink: &str,
    font_px: f32,
    code_family: &str,
) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\
         <title>{nonce}</title>\
         <style>@font-face{{font-family:\"tidemail-code\";src:url(\"{CID_ORIGIN}{FONT_PATH}{code_url}\");font-weight:100 900}}\
         html{{color-scheme:light;background:{paper};color:{ink};font:{font_px}px/1.5 \"Segoe UI Variable Text\",\"Segoe UI\",sans-serif;\
         overflow-wrap:break-word}}body{{margin:16px}}img{{max-width:100%;height:auto}}\
         code,pre,kbd,samp,tt{{font-family:\"tidemail-code\",ui-monospace,Consolas,monospace;font-size:0.92em}}</style>\
         </head><body>{html}</body></html>",
        csp = csp(load_images),
        code_url = code_family.replace(' ', "%20"),
    )
}

/// The family a [`FONT_PATH`] request asks for.
pub fn parse_font_path(path: &str) -> Option<String> {
    percent_decode(path.strip_prefix(FONT_PATH)?)
}

/// Host script (`ExecuteScript` runs even with page scripts off) that puts
/// `html` into the current document and returns `nonce`. The document's CSP
/// meta tag stays in force, so this is only used when the image policy is
/// unchanged. A full navigation costs ~70 ms per j/k step; this costs a few.
pub fn swap_script(html: &str, nonce: u64) -> String {
    let html = serde_json::to_string(html).expect("a string serializes");
    format!("document.title='{nonce}';document.body.innerHTML={html};scrollTo(0,0);document.title")
}

/// True when the HTML refers to remote images (`src="http..."`, `url(http...)`).
/// Drives the "Load images" button; the CSP is what blocks them.
pub fn has_remote_images(html: &str) -> bool {
    // Inline `cid:` images are rewritten to CID_ORIGIN and always allowed.
    let lower = html.replace(CID_ORIGIN, "").to_ascii_lowercase();
    ["src=\"http", "src='http", "src=http", "url(http", "url('http", "url(\"http"]
        .iter()
        .any(|p| lower.contains(p))
}

/// `/<message_id>/<percent-encoded cid>` of a `tidemail-cid` request.
pub fn parse_cid_path(path: &str) -> Option<(i64, String)> {
    let (message, cid) = path.strip_prefix('/')?.split_once('/')?;
    Some((message.parse().ok()?, percent_decode(cid)?))
}

fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hi = (bytes.next()? as char).to_digit(16)?;
            let lo = (bytes.next()? as char).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok()
}

/// Only `http`, `https` and `mailto` leave the app.
pub fn is_external(url: &str) -> bool {
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| url.len() >= p.len() && url[..p.len()].eq_ignore_ascii_case(p))
}

/// "12 KB" style size for the attachment list.
pub fn size(bytes: u32) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", bytes / 1024),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_script_embeds_the_body_as_a_js_string() {
        let s = swap_script("<p class=\"a\">it's</p>\n", 7);
        assert_eq!(
            s,
            "document.title='7';document.body.innerHTML=\"<p class=\\\"a\\\">it's</p>\\n\";scrollTo(0,0);document.title"
        );
    }

    #[test]
    fn csp_adds_https_images_only_when_allowed() {
        assert!(!csp(false).contains("https:"));
        assert!(csp(true).contains("img-src data: http://tidemail-cid.localhost https:"));
        assert!(csp(false).starts_with("default-src 'none'"));
        assert!(csp(false).ends_with("font-src data: http://tidemail-cid.localhost"));
    }

    #[test]
    fn detects_remote_images() {
        assert!(has_remote_images("<p><IMG SRC=\"https://x.test/a.png\"></p>"));
        assert!(has_remote_images("<div style=\"background:url(http://x.test/a)\">"));
        assert!(!has_remote_images(
            "<img src=\"http://tidemail-cid.localhost/1/a\">"
        ));
        assert!(!has_remote_images("<a href=\"https://x.test\">link</a>"));
    }

    #[test]
    fn parses_cid_paths() {
        assert_eq!(
            parse_cid_path("/42/logo%40mail.test"),
            Some((42, "logo@mail.test".into()))
        );
        assert_eq!(parse_cid_path("/x/a"), None);
        assert_eq!(parse_cid_path("/1/%zz"), None);
        assert_eq!(parse_cid_path("/1"), None);
        // Font requests share the origin and never parse as a cid.
        assert_eq!(parse_cid_path("/font/Obel%20Mono"), None);
        assert_eq!(parse_font_path("/font/Obel%20Mono"), Some("Obel Mono".into()));
        assert_eq!(parse_font_path("/42/a"), None);
    }

    #[test]
    fn external_schemes() {
        assert!(is_external("https://a.test"));
        assert!(is_external("MAILTO:a@b.test"));
        assert!(!is_external("javascript:alert(1)"));
        assert!(!is_external("file:///c:/x"));
        assert!(!is_external("http"));
    }

    #[test]
    fn document_carries_nonce_and_csp() {
        let d = document("<p>hi</p>", 7, false, "#fff", "#000", 15.0, "Obel Mono Masked");
        assert!(d.contains("<title>7</title>"));
        assert!(d.contains("url(\"http://tidemail-cid.localhost/font/Obel%20Mono%20Masked\")"));
        assert!(d.contains("Content-Security-Policy"));
        assert!(d.contains("<p>hi</p>"));
    }
}
