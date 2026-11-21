//! Mail rendering: HTML sanitizing at ingest, `cid:` image lookup,
//! attachment files with Mark-of-the-Web.
//!
//! `sanitize` runs ammonia at ingest (`SANITIZER_VERSION` 2); see its docs for the rules.
//! `save_attachment_in` is called by the sync engine after a download.

use std::path::{Path, PathBuf};

use tidemail_db::rusqlite::{params, Connection, OptionalExtension};

/// Bump on every sanitizer change; a background job re-sanitizes old rows.
pub const SANITIZER_VERSION: u32 = 2;

/// Custom URI scheme for inline images, registered by the app crate.
pub const CID_SCHEME: &str = "tidemail-cid";

/// Origin that WebView2 uses for `CID_SCHEME`. On Windows, Tauri serves a
/// custom scheme as `http://<scheme>.localhost/`, not as `<scheme>://`.
/// `cid:` URLs are rewritten to `{CID_ORIGIN}/<message_id>/<percent-encoded content_id>`.
pub const CID_ORIGIN: &str = "http://tidemail-cid.localhost";

/// Sanitize one HTML body for storage in `bodies.html_sanitized`.
///
/// Removes scripts, forms, frames, event handlers and unsafe URL schemes.
/// Keeps `<style>` and `style` attributes (the iframe CSP is the safety net
/// for CSS). Every link gets `target="_blank" rel="noopener noreferrer"`.
/// `cid:` image URLs become `{CID_ORIGIN}/<message_id>/<percent-encoded cid>`.
/// `img src` keeps only http, https, data and cid; links lose cid and data.
pub fn sanitize(html: &str, message_id: i64) -> String {
    let mut builder = ammonia::Builder::default();
    builder
        .link_rel(Some("noopener noreferrer"))
        .add_tags(["style", "center", "font"])
        .rm_clean_content_tags(["style"])
        .add_clean_content_tags(["title"])
        .add_generic_attributes([
            "style",
            "class",
            "dir",
            "align",
            "valign",
            "bgcolor",
            "width",
            "height",
            "border",
            "cellpadding",
            "cellspacing",
        ])
        .add_tag_attributes("font", ["color", "size", "face"])
        .set_tag_attribute_value("a", "target", "_blank")
        .url_schemes(["http", "https", "mailto", "tel", "cid", "data"].into_iter().collect())
        .attribute_filter(move |element, attribute, value| {
            if !matches!((element, attribute), ("img" | "a", "src" | "href")) {
                return Some(value.into());
            }
            let scheme = value
                .split_once(':')
                .map(|(s, _)| s.trim().to_ascii_lowercase());
            match (element, scheme.as_deref()) {
                ("img", Some("cid")) => {
                    let cid = &value.trim_start()[4..];
                    Some(format!("{CID_ORIGIN}/{message_id}/{}", percent_encode(cid)).into())
                }
                ("img", _) => Some(value.into()),
                (_, Some("cid" | "data")) => None,
                _ => Some(value.into()),
            }
        });
    builder.clean(html).to_string()
}

/// Percent-encode everything except RFC 3986 unreserved characters.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Plain text from HTML, for FTS and the plain-text fallback.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 3);
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let rest = &lower[i..];
            let skip_to = ["script", "style", "head"].iter().find_map(|t| {
                rest.strip_prefix('<')
                    .filter(|r| {
                        r.starts_with(t)
                            && r[t.len()..].starts_with(|c: char| c == '>' || c.is_whitespace())
                    })
                    .and_then(|_| rest.find(&format!("</{t}")).map(|p| i + p))
            });
            if let Some(end) = skip_to {
                i = end;
            }
            let close = lower[i..].find('>').map_or(bytes.len(), |p| i + p + 1);
            let tag = &lower[i..close];
            if ["<br", "<p", "</p", "<div", "</div", "<tr", "<li", "<h"]
                .iter()
                .any(|t| tag.starts_with(t))
            {
                out.push('\n');
            } else {
                out.push(' ');
            }
            i = close;
        } else {
            let next = lower[i..].find('<').map_or(bytes.len(), |p| i + p);
            out.push_str(&decode_entities(&html[i..next]));
            i = next;
        }
    }
    // collapse runs of blank space
    let mut text = String::with_capacity(out.len());
    let mut blank = 0;
    for line in out
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
    {
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        text.push_str(&line);
        text.push('\n');
    }
    text.trim().to_owned()
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Bytes and MIME type of an inline `cid:` part, from `inline_blobs`.
pub fn load_cid(
    conn: &Connection,
    message_id: i64,
    content_id: &str,
) -> tidemail_db::Result<Option<(String, Vec<u8>)>> {
    conn.prepare_cached(
        "SELECT a.mime_type, b.data FROM attachments a JOIN inline_blobs b ON b.attachment_id = a.id \
         WHERE a.message_id = ?1 AND a.content_id = ?2",
    )?
    .query_row(params![message_id, content_id], |r| Ok((r.get(0)?, r.get(1)?)))
    .optional()
}

/// `<base>\attachments\<account>\<message>\<safe name>`.
/// `base` is the Tidemail data folder (`%LOCALAPPDATA%\Tidemail`).
pub fn attachment_path(base: &Path, account_id: i64, message_id: i64, filename: &str) -> PathBuf {
    base.join("attachments")
        .join(account_id.to_string())
        .join(message_id.to_string())
        .join(safe_filename(filename))
}

/// Make a sender-supplied file name safe as one Windows path component.
/// Drops path parts, replaces reserved and control characters, avoids device
/// names (`CON`, `NUL`, ...), trailing dots and spaces, and caps the length.
pub fn safe_filename(name: &str) -> String {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or("");
    let mut clean: String = leaf
        .chars()
        .map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c })
        .take(120)
        .collect();
    clean.truncate(clean.trim_end_matches(['.', ' ']).len());
    let stem = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit() && c != '0'));
    if clean.trim().is_empty() {
        "attachment".to_owned()
    } else if reserved {
        format!("_{clean}")
    } else {
        clean
    }
}

/// Write an attachment and its `Zone.Identifier` stream (Mark-of-the-Web,
/// `ZoneId=3` = Internet). Creates parent folders. The stream needs NTFS.
pub fn save_attachment(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, data)?;
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(stream, b"[ZoneTransfer]\r\nZoneId=3\r\n")
}

/// Build the attachment path, save the file with Mark-of-the-Web, return the path.
/// The sync engine calls this after it downloads the bytes.
pub fn save_attachment_in(
    base: &Path,
    account_id: i64,
    message_id: i64,
    filename: &str,
    data: &[u8],
) -> std::io::Result<PathBuf> {
    let path = attachment_path(base, account_id, message_id, filename);
    save_attachment(&path, data)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script_and_style() {
        let t = html_to_text(
            "<html><head><title>x</title></head><style>p{}</style><p>Hello&nbsp;<b>you</b></p><script>bad()</script><p>Bye</p>",
        );
        assert_eq!(t, "Hello you\n\nBye");
    }

    #[test]
    fn sanitize_removes_script() {
        let s = sanitize("<p onclick=x>Hi</p><script>alert(1)</script>", 1);
        assert!(!s.contains("script") && !s.contains("onclick"));
    }

    const HOSTILE: &[(&str, &str)] = &[
        ("script", include_str!("../../bench/fixtures/hostile/script.html")),
        ("event-handlers", include_str!("../../bench/fixtures/hostile/event-handlers.html")),
        ("javascript-links", include_str!("../../bench/fixtures/hostile/javascript-links.html")),
        ("forms-frames", include_str!("../../bench/fixtures/hostile/forms-frames.html")),
        ("mutation-xss", include_str!("../../bench/fixtures/hostile/mutation-xss.html")),
        ("css-overlay", include_str!("../../bench/fixtures/hostile/css-overlay.html")),
        ("cid-images", include_str!("../../bench/fixtures/hostile/cid-images.html")),
        ("tracking", include_str!("../../bench/fixtures/hostile/tracking.html")),
        ("spoofing", include_str!("../../bench/fixtures/hostile/spoofing.html")),
        ("wide-table", include_str!("../../bench/fixtures/hostile/wide-table.html")),
    ];

    #[test]
    fn hostile_fixtures_lose_active_content() {
        for (name, html) in HOSTILE {
            let s = sanitize(html, 7).to_ascii_lowercase();
            for bad in ["<script", "<form", "<iframe", "<object", "<embed", "<svg", "<math", "<input", "javascript:", "onerror", "onclick", "onload", "<template", "<link", "<meta"] {
                assert!(!s.contains(bad), "{name}: still contains {bad}\n{s}");
            }
        }
    }

    #[test]
    fn keeps_style_element_and_attribute() {
        let s = sanitize("<style>p{color:red}</style><p style=\"color:blue\" class=\"x\">a</p>", 1);
        assert!(s.contains("<style>p{color:red}</style>"), "{s}");
        assert!(s.contains("style=\"color:blue\""), "{s}");
    }

    #[test]
    fn links_open_in_new_window_with_rel() {
        let s = sanitize(r#"<a href="https://a.example/" target="_self">x</a><a href="mailto:a@b.c">m</a>"#, 1);
        assert_eq!(s.matches(r#"target="_blank""#).count(), 2, "{s}");
        assert_eq!(s.matches("noopener noreferrer").count(), 2, "{s}");
    }

    #[test]
    fn rewrites_cid_images() {
        let s = sanitize(r#"<img src="cid:image001.png@01DA0001"><img src="CID:a b"><a href="cid:x">l</a>"#, 42);
        assert!(s.contains("src=\"http://tidemail-cid.localhost/42/image001.png%4001DA0001\""), "{s}");
        assert!(s.contains("/42/a%20b\""), "{s}");
        assert!(!s.contains("cid:"), "{s}");
    }

    #[test]
    fn drops_other_img_schemes() {
        let s = sanitize(r#"<img src="file:///c:/x.png"><img src="ftp://h/x.png"><img src="data:image/png;base64,AAAA"><img src="https://t.example/p.gif">"#, 1);
        assert!(!s.contains("file:") && !s.contains("ftp:"), "{s}");
        assert!(s.contains("data:image/png;base64,AAAA") && s.contains("https://t.example/p.gif"), "{s}");
    }

    #[test]
    fn large_newsletter_is_fast() {
        let row = r#"<table width="600"><tr><td style="padding:4px"><a href="https://n.example/x?utm=1">Read more</a><img src="https://t.example/p.gif" width="1" height="1"><p onclick="x()">Lorem ipsum dolor sit amet</p></td></tr></table>"#;
        let html = row.repeat(1_000_000 / row.len() + 1);
        let t = std::time::Instant::now();
        let s = sanitize(&html, 1);
        let ms = t.elapsed().as_millis();
        eprintln!("sanitize {} bytes in {ms} ms", html.len());
        assert!(!s.contains("onclick"));
        assert!(ms < 5_000, "{ms} ms");
    }

    #[test]
    fn safe_filenames() {
        assert_eq!(safe_filename(r"..\..\evil/a:b?.pdf"), "a_b_.pdf");
        assert_eq!(safe_filename("CON.txt"), "_CON.txt");
        assert_eq!(safe_filename("com1"), "_com1");
        assert_eq!(safe_filename("report. "), "report");
        assert_eq!(safe_filename(""), "attachment");
        assert_eq!(safe_filename("..."), "attachment");
        assert_eq!(safe_filename("ok name (1).pdf"), "ok name (1).pdf");
        assert_eq!(safe_filename(&"é".repeat(300)).chars().count(), 120);
    }

    #[test]
    fn saves_with_mark_of_the_web() {
        let base = std::env::temp_dir().join(format!("tidemail-render-test-{}", std::process::id()));
        let path = save_attachment_in(&base, 1, 2, "a<b>.txt", b"hello").unwrap();
        assert_eq!(path, base.join("attachments").join("1").join("2").join("a_b_.txt"));
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        #[cfg(windows)]
        {
            let mut ads = path.as_os_str().to_owned();
            ads.push(":Zone.Identifier");
            assert_eq!(std::fs::read(ads).unwrap(), b"[ZoneTransfer]\r\nZoneId=3\r\n");
        }
        std::fs::remove_dir_all(&base).unwrap();
    }
}
