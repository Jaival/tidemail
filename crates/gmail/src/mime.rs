//! MIME helpers for `format=full` payloads.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

use crate::{Error, Payload};

/// Decode Gmail's base64url (padding tolerated).
pub fn decode_base64url(data: &str) -> Result<Vec<u8>, Error> {
    URL_SAFE_NO_PAD
        .decode(data.trim_end_matches('='))
        .map_err(|e| Error::Parse(format!("base64url: {e}")))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bodies {
    pub html: Option<String>,
    pub text: Option<String>,
}

/// An attachment or inline (`cid:`) part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentInfo {
    pub part_id: Option<String>,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    /// For `attachments.get`. `None` when the data is inline in the payload.
    pub attachment_id: Option<String>,
    /// `Content-ID` without angle brackets; referenced by `cid:` URLs.
    pub content_id: Option<String>,
}

fn is_body_candidate(p: &Payload, mime: &str) -> bool {
    p.mime_type.eq_ignore_ascii_case(mime) && p.filename.is_empty() && p.parts.is_empty()
}

fn charset(p: &Payload) -> Option<&str> {
    let ct = p.header("Content-Type")?;
    let idx = ct.to_ascii_lowercase().find("charset=")? + "charset=".len();
    Some(ct[idx..].split(';').next()?.trim().trim_matches('"'))
}

fn decode_part(p: &Payload) -> Result<Option<String>, Error> {
    let Some(data) = p.body.data.as_deref() else {
        return Ok(None);
    };
    let bytes = decode_base64url(data)?;
    let enc = charset(p)
        .and_then(|c| encoding_rs::Encoding::for_label(c.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    Ok(Some(enc.decode(&bytes).0.into_owned()))
}

fn find<'a>(p: &'a Payload, mime: &str) -> Option<&'a Payload> {
    if is_body_candidate(p, mime) {
        return Some(p);
    }
    p.parts.iter().find_map(|c| find(c, mime))
}

/// First `text/html` and first `text/plain` body part (attachments excluded).
pub fn extract_bodies(root: &Payload) -> Result<Bodies, Error> {
    Ok(Bodies {
        html: find(root, "text/html")
            .map(decode_part)
            .transpose()?
            .flatten(),
        text: find(root, "text/plain")
            .map(decode_part)
            .transpose()?
            .flatten(),
    })
}

fn collect(p: &Payload, out: &mut Vec<AttachmentInfo>) {
    if !p.parts.is_empty() {
        for c in &p.parts {
            collect(c, out);
        }
        return;
    }
    let content_id = p.header("Content-ID").map(|v| {
        v.trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string()
    });
    // Plain body parts (no filename, no Content-ID) are not attachments.
    if p.filename.is_empty() && content_id.is_none() {
        return;
    }
    out.push(AttachmentInfo {
        part_id: p.part_id.clone(),
        filename: p.filename.clone(),
        mime_type: p.mime_type.clone(),
        size: p.body.size,
        attachment_id: p.body.attachment_id.clone(),
        content_id,
    });
}

/// Attachments and inline `cid:` parts, with sizes.
pub fn list_attachments(root: &Payload) -> Vec<AttachmentInfo> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn b64(s: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(s)
    }

    fn payload() -> Payload {
        serde_json::from_value(json!({
            "mimeType": "multipart/mixed",
            "parts": [
                {"partId": "0", "mimeType": "multipart/alternative", "parts": [
                    {"partId": "0.0", "mimeType": "text/plain",
                     "headers": [{"name": "Content-Type", "value": "text/plain; charset=iso-8859-1"}],
                     "body": {"size": 4, "data": b64(&[0x63, 0x61, 0x66, 0xE9])}},
                    {"partId": "0.1", "mimeType": "multipart/related", "parts": [
                        {"partId": "0.1.0", "mimeType": "text/html",
                         "headers": [{"name": "Content-Type", "value": "text/html; charset=\"UTF-8\""}],
                         "body": {"size": 20, "data": b64("<p>héllo <img src=cid:logo></p>".as_bytes())}},
                        {"partId": "0.1.1", "mimeType": "image/png", "filename": "logo.png",
                         "headers": [{"name": "Content-ID", "value": "<logo>"}],
                         "body": {"size": 1234, "attachmentId": "att-logo"}}
                    ]}
                ]},
                {"partId": "1", "mimeType": "application/pdf", "filename": "bill.pdf",
                 "body": {"size": 99999, "attachmentId": "att-pdf"}},
                {"partId": "2", "mimeType": "text/plain", "filename": "notes.txt",
                 "body": {"size": 5, "attachmentId": "att-txt"}}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn decodes_base64url_with_and_without_padding() {
        assert_eq!(decode_base64url("aGk_").unwrap(), b"hi?");
        assert_eq!(decode_base64url("aGk=").unwrap(), b"hi");
        assert!(decode_base64url("!!").is_err());
    }

    #[test]
    fn picks_bodies_honouring_charset_and_skipping_attachments() {
        let b = extract_bodies(&payload()).unwrap();
        assert_eq!(b.text.as_deref(), Some("café"));
        assert!(b.html.unwrap().starts_with("<p>héllo"));
    }

    #[test]
    fn lists_attachments_and_inline_parts() {
        let a = list_attachments(&payload());
        let names: Vec<_> = a.iter().map(|x| x.filename.as_str()).collect();
        assert_eq!(names, ["logo.png", "bill.pdf", "notes.txt"]);
        assert_eq!(a[0].content_id.as_deref(), Some("logo"));
        assert_eq!(a[0].size, 1234);
        assert_eq!(a[1].attachment_id.as_deref(), Some("att-pdf"));
        assert_eq!(a[1].content_id, None);
    }

    #[test]
    fn single_part_message() {
        let p: Payload = serde_json::from_value(json!({
            "mimeType": "text/plain", "body": {"size": 2, "data": b64(b"ok")}
        }))
        .unwrap();
        let b = extract_bodies(&p).unwrap();
        assert_eq!(b.text.as_deref(), Some("ok"));
        assert_eq!(b.html, None);
        assert!(list_attachments(&p).is_empty());
    }
}
