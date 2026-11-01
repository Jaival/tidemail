//! Mail rendering: HTML sanitizing at ingest, `cid:` image lookup,
//! attachment files with Mark-of-the-Web.
//!
//! Phase 0 state: `sanitize` is a basic ammonia pass and `html_to_text` a
//! simple tag stripper. Phase 1 (renderer agent) owns this crate: allow
//! `<style>` + `style`, rewrite `cid:` to `tidemail-cid://`, link attrs,
//! Mark-of-the-Web. Keep the public signatures below.

use std::path::{Path, PathBuf};

use tidemail_db::rusqlite::{params, Connection, OptionalExtension};

/// Bump on every sanitizer change; a background job re-sanitizes old rows.
pub const SANITIZER_VERSION: u32 = 1;

/// URI scheme for inline images: `tidemail-cid://<message_id>/<content_id>`.
pub const CID_SCHEME: &str = "tidemail-cid";

/// Sanitize one HTML body for storage in `bodies.html_sanitized`.
/// `message_id` is used to rewrite `cid:` URLs.
pub fn sanitize(html: &str, message_id: i64) -> String {
    let _ = message_id; // Phase 1: cid: rewrite
    ammonia::Builder::default()
        .link_rel(Some("noopener noreferrer"))
        .clean(html)
        .to_string()
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

/// `<base>\attachments\<account>\<message>\<name>`.
pub fn attachment_path(base: &Path, account_id: i64, message_id: i64, filename: &str) -> PathBuf {
    // Phase 1: sanitize `filename` for Windows.
    base.join("attachments")
        .join(account_id.to_string())
        .join(message_id.to_string())
        .join(filename)
}

/// Write an attachment and its `Zone.Identifier` stream (Mark-of-the-Web).
pub fn save_attachment(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let _ = (path, data);
    Err(std::io::Error::other("not implemented (Phase 1: renderer)"))
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
}
