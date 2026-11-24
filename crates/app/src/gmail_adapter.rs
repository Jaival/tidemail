//! Adapter from `tidemail_gmail` to the sync engine's `GmailApi` / `GmailProvider` traits.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use tidemail_gmail as gm;
use tidemail_sync::api::{
    ApiError, ApiResult, Format, GmailApi, GmailProvider, HistoryPage, IdPage, MessageRef, Profile,
    RemoteAttachment, RemoteBody, RemoteLabel, RemoteMessage, SignedInAccount,
};

/// Rounds of resubmitting per-part rate-limited batch calls before giving up.
/// Waits 2, 4, … 60 s: two minutes, so a per-minute quota window passes.
const BATCH_RETRIES: u32 = 6;
/// Parts per HTTP batch call. Gmail runs the parts of a batch at once and
/// answers 429 "too many concurrent requests" to most parts of a batch of 50.
const BATCH_PARTS: usize = 10;

fn map_err(e: gm::Error) -> ApiError {
    match e {
        gm::Error::Http(e) => ApiError::Offline(e.to_string()),
        gm::Error::AuthExpired | gm::Error::Auth(_) => ApiError::Auth(e.to_string()),
        gm::Error::HistoryExpired => ApiError::HistoryExpired,
        gm::Error::RateLimited => ApiError::RateLimited,
        gm::Error::Config(e) => ApiError::Setup(e),
        e => ApiError::Other(e.to_string()),
    }
}

/// Part bytes when Gmail returned them inline in the payload.
fn inline_data(root: &gm::Payload, part_id: &str) -> Option<Vec<u8>> {
    if root.part_id.as_deref().unwrap_or("") == part_id {
        return gm::decode_base64url(root.body.data.as_deref()?).ok();
    }
    root.parts.iter().find_map(|p| inline_data(p, part_id))
}

fn to_remote(m: gm::Message, format: Format) -> Result<RemoteMessage, gm::Error> {
    let header = |name: &str| m.header(name).unwrap_or_default().to_string();
    let body = match (format, &m.payload) {
        (Format::Full, Some(root)) => {
            let bodies = gm::extract_bodies(root)?;
            let attachments: Vec<_> = gm::list_attachments(root)
                .into_iter()
                .map(|a| {
                    let part_id = a.part_id.unwrap_or_default();
                    RemoteAttachment {
                        data: inline_data(root, &part_id),
                        part_id,
                        attachment_id: a.attachment_id,
                        filename: a.filename,
                        mime_type: a.mime_type,
                        size: u32::try_from(a.size).unwrap_or(u32::MAX),
                        is_inline: a.content_id.is_some(),
                        content_id: a.content_id,
                    }
                })
                .collect();
            Some(RemoteBody {
                html: bodies.html,
                text: bodies.text,
                attachments,
            })
        }
        _ => None,
    };
    Ok(RemoteMessage {
        from: header("From"),
        to: header("To"),
        cc: header("Cc"),
        subject: header("Subject"),
        has_attachment: body
            .as_ref()
            .is_some_and(|b| b.attachments.iter().any(|a| !a.is_inline)),
        body,
        internal_date_ms: m.internal_date.unwrap_or_default(),
        id: m.id,
        thread_id: m.thread_id,
        label_ids: m.label_ids,
        snippet: unescape_snippet(&m.snippet),
    })
}

/// Gmail sends `snippet` HTML-escaped (`Don&#39;t`). The list shows plain text.
fn unescape_snippet(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let c = match &rest[1..end] {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => '\u{a0}',
                n => match n.strip_prefix("#x").or_else(|| n.strip_prefix("#X")) {
                    Some(hex) => char::from_u32(u32::from_str_radix(hex, 16).ok()?)?,
                    None => char::from_u32(n.strip_prefix('#')?.parse().ok()?)?,
                },
            };
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn to_history_page(h: gm::HistoryList) -> ApiResult<HistoryPage> {
    let ids = |v: &[gm::MessageAdded]| v.iter().map(|m| m.message.id.clone()).collect::<Vec<_>>();
    let changed =
        |v: &[gm::LabelChange]| v.iter().map(|m| m.message.id.clone()).collect::<Vec<_>>();
    let mut page = HistoryPage {
        next_page_token: h.next_page_token,
        history_id: h
            .history_id
            .ok_or_else(|| ApiError::Other("history.list response has no historyId".into()))?,
        ..HistoryPage::default()
    };
    for r in &h.history {
        page.added.extend(ids(&r.messages_added));
        page.deleted.extend(ids(&r.messages_deleted));
        page.label_changed.extend(changed(&r.labels_added));
        page.label_changed.extend(changed(&r.labels_removed));
    }
    Ok(page)
}

fn to_label(l: gm::Label) -> RemoteLabel {
    RemoteLabel {
        is_system: l.kind == "system",
        id: l.id,
        name: l.name,
        color: None,
    }
}

struct Api(gm::GmailClient);

#[async_trait]
impl GmailApi for Api {
    async fn get_profile(&self) -> ApiResult<Profile> {
        let p = self.0.profile().await.map_err(map_err)?;
        Ok(Profile {
            email: p.email_address,
            history_id: p.history_id,
            messages_total: p.messages_total,
        })
    }

    async fn list_message_ids(
        &self,
        label: Option<&str>,
        query: Option<&str>,
        page_token: Option<&str>,
    ) -> ApiResult<IdPage> {
        let labels: Vec<&str> = label.into_iter().collect();
        let list = self
            .0
            .list_messages(query, &labels, page_token, None)
            .await
            .map_err(map_err)?;
        Ok(IdPage {
            ids: list
                .messages
                .into_iter()
                .map(|m| MessageRef {
                    id: m.id,
                    thread_id: m.thread_id,
                })
                .collect(),
            next_page_token: list.next_page_token,
        })
    }

    async fn get_messages(&self, ids: &[String], format: Format) -> ApiResult<Vec<RemoteMessage>> {
        let fmt = match format {
            Format::Metadata => gm::MessageFormat::Metadata,
            Format::Full => gm::MessageFormat::Full,
        };
        let requests: Vec<_> = ids
            .iter()
            .map(|id| gm::BatchRequest::get_message(id, fmt))
            .collect();
        let mut results: Vec<Option<gm::BatchResponse>> = vec![None; ids.len()];
        let mut pending: Vec<usize> = (0..ids.len()).collect();
        let mut reason = String::new();
        for round in 0..=BATCH_RETRIES {
            let mut responses = Vec::with_capacity(pending.len());
            for part in pending.chunks(BATCH_PARTS) {
                let batch: Vec<_> = part.iter().map(|&i| requests[i].clone()).collect();
                responses.extend(self.0.batch(&batch).await.map_err(map_err)?);
            }
            let mut limited = Vec::new();
            for (&i, r) in pending.iter().zip(responses) {
                if r.is_rate_limited() {
                    reason = format!("{} {}", r.status, r.body["error"]["message"]);
                    limited.push(i);
                } else {
                    results[i] = Some(r);
                }
            }
            pending = limited;
            if pending.is_empty() {
                break;
            }
            if std::env::var_os("TIDEMAIL_SYNC_LOG").is_some() {
                eprintln!(
                    "tidemail: batch round {round}: {} of {} parts rate limited ({reason})",
                    pending.len(),
                    ids.len()
                );
            }
            if round == BATCH_RETRIES {
                eprintln!(
                    "tidemail: {} of {} batch parts still rate limited ({})",
                    pending.len(),
                    ids.len(),
                    reason
                );
                return Err(ApiError::RateLimited);
            }
            tokio::time::sleep(Duration::from_secs((2u64 << round).min(60))).await;
        }
        let mut out = Vec::with_capacity(ids.len());
        for r in results.into_iter().flatten() {
            if r.status == 404 {
                continue;
            }
            let m: gm::Message = r.into_result().map_err(map_err)?;
            out.push(to_remote(m, format).map_err(map_err)?);
        }
        Ok(out)
    }

    async fn get_attachment(&self, message_id: &str, attachment_id: &str) -> ApiResult<Vec<u8>> {
        let a = self
            .0
            .get_attachment(message_id, attachment_id)
            .await
            .map_err(map_err)?;
        gm::decode_base64url(&a.data).map_err(map_err)
    }

    async fn list_history(
        &self,
        start_history_id: &str,
        page_token: Option<&str>,
    ) -> ApiResult<HistoryPage> {
        let h = self
            .0
            .list_history(start_history_id, page_token)
            .await
            .map_err(map_err)?;
        to_history_page(h)
    }

    async fn list_labels(&self) -> ApiResult<Vec<RemoteLabel>> {
        let l = self.0.list_labels().await.map_err(map_err)?;
        Ok(l.labels.into_iter().map(to_label).collect())
    }

    async fn threads_modify(
        &self,
        thread_id: &str,
        add_labels: &[String],
        remove_labels: &[String],
    ) -> ApiResult<()> {
        let add: Vec<&str> = add_labels.iter().map(String::as_str).collect();
        let remove: Vec<&str> = remove_labels.iter().map(String::as_str).collect();
        self.0
            .modify_thread(thread_id, &add, &remove)
            .await
            .map(drop)
            .map_err(map_err)
    }

    async fn threads_trash(&self, thread_id: &str) -> ApiResult<()> {
        self.0
            .trash_thread(thread_id)
            .await
            .map(drop)
            .map_err(map_err)
    }

    async fn threads_untrash(&self, thread_id: &str) -> ApiResult<()> {
        self.0
            .untrash_thread(thread_id)
            .await
            .map(drop)
            .map_err(map_err)
    }

    async fn messages_send(&self, raw: &[u8], thread_id: Option<&str>) -> ApiResult<MessageRef> {
        let m = self
            .0
            .send_message(raw, thread_id)
            .await
            .map_err(map_err)?;
        Ok(MessageRef {
            id: m.id,
            thread_id: m.thread_id,
        })
    }
}

/// Real provider. `config.toml` is read on first use, not at launch; a
/// failed read is retried next time (the user may create the file).
pub struct Provider {
    config: Mutex<Option<gm::Config>>,
    apis: Mutex<HashMap<String, Arc<dyn GmailApi>>>,
}

impl Provider {
    pub fn new() -> Self {
        Self {
            config: Mutex::default(),
            apis: Mutex::default(),
        }
    }

    fn config(&self) -> ApiResult<gm::Config> {
        let mut config = self.config.lock().expect("config lock");
        if let Some(c) = config.as_ref() {
            return Ok(c.clone());
        }
        let loaded = gm::load_config().map_err(map_err)?;
        *config = Some(loaded.clone());
        Ok(loaded)
    }
}

#[async_trait]
impl GmailProvider for Provider {
    async fn sign_in(&self) -> ApiResult<SignedInAccount> {
        let config = self.config()?;
        let opener = gm::open_in_browser;
        let signed = gm::sign_in_with(
            &config,
            &gm::Endpoints::default(),
            &gm::KeyringStore,
            opener,
        )
        .await
        .map_err(map_err)?;
        Ok(SignedInAccount {
            email: signed.email,
            display_name: signed.display_name,
        })
    }

    fn api(&self, email: &str) -> ApiResult<Arc<dyn GmailApi>> {
        let mut apis = self.apis.lock().expect("api cache lock");
        if let Some(api) = apis.get(email) {
            return Ok(api.clone());
        }
        let tokens = gm::TokenSource::new(
            self.config()?,
            gm::Endpoints::default().token_url,
            email.to_string(),
            Arc::new(gm::KeyringStore),
        );
        let api: Arc<dyn GmailApi> = Arc::new(Api(gm::GmailClient::new(Arc::new(tokens))));
        apis.insert(email.to_string(), api.clone());
        Ok(api)
    }

    fn sign_out(&self, email: &str) -> ApiResult<()> {
        self.apis.lock().expect("api cache lock").remove(email);
        gm::sign_out(email).map_err(map_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snippet_entities_are_decoded() {
        assert_eq!(
            unescape_snippet("Don&#39;t &amp; &lt;b&gt; &quot;x&quot; &#x263A; caf&eacute; AT&T &;"),
            "Don't & <b> \"x\" \u{263A} caf&eacute; AT&T &;"
        );
    }

    #[test]
    fn maps_full_message() {
        let m: gm::Message = serde_json::from_value(json!({
            "id": "m1", "threadId": "t1", "labelIds": ["INBOX", "UNREAD"],
            "snippet": "hi", "internalDate": "1700000000000",
            "payload": {
                "partId": "", "mimeType": "multipart/mixed",
                "headers": [
                    {"name": "from", "value": "\"A\" <a@x.y>"},
                    {"name": "To", "value": "b@x.y"},
                    {"name": "Subject", "value": "Hello"}
                ],
                "parts": [
                    {"partId": "0", "mimeType": "text/plain", "body": {"size": 2, "data": "aGk"}},
                    {"partId": "1", "mimeType": "image/png", "filename": "p.png",
                     "headers": [{"name": "Content-ID", "value": "<cid1>"}],
                     "body": {"size": 3, "data": "AQID"}},
                    {"partId": "2", "mimeType": "application/pdf", "filename": "a.pdf",
                     "body": {"size": 9, "attachmentId": "att1"}}
                ]
            }
        }))
        .unwrap();
        let r = to_remote(m.clone(), Format::Full).unwrap();
        assert_eq!(r.from, "\"A\" <a@x.y>");
        assert_eq!(r.to, "b@x.y");
        assert_eq!(r.cc, "");
        assert_eq!(r.subject, "Hello");
        assert_eq!(r.internal_date_ms, 1_700_000_000_000);
        assert_eq!(r.label_ids, ["INBOX", "UNREAD"]);
        assert!(r.has_attachment);
        let body = r.body.unwrap();
        assert_eq!(body.text.as_deref(), Some("hi"));
        let png = body
            .attachments
            .iter()
            .find(|a| a.filename == "p.png")
            .unwrap();
        assert_eq!(png.content_id.as_deref(), Some("cid1"));
        assert!(png.is_inline);
        assert_eq!(png.data.as_deref(), Some(&[1u8, 2, 3][..]));
        let pdf = body
            .attachments
            .iter()
            .find(|a| a.filename == "a.pdf")
            .unwrap();
        assert_eq!(pdf.attachment_id.as_deref(), Some("att1"));
        assert!(pdf.data.is_none());
        assert!(to_remote(m, Format::Metadata).unwrap().body.is_none());
    }

    #[test]
    fn maps_history() {
        let h: gm::HistoryList = serde_json::from_value(json!({
            "history": [
                {"id": "1", "messagesAdded": [{"message": {"id": "a", "threadId": "t"}}]},
                {"id": "2", "messagesDeleted": [{"message": {"id": "b", "threadId": "t"}}],
                 "labelsAdded": [{"message": {"id": "c", "threadId": "t"}, "labelIds": ["X"]}],
                 "labelsRemoved": [{"message": {"id": "d", "threadId": "t"}, "labelIds": ["Y"]}]}
            ],
            "nextPageToken": "n", "historyId": "99"
        }))
        .unwrap();
        let p = to_history_page(h).unwrap();
        assert_eq!(p.added, ["a"]);
        assert_eq!(p.deleted, ["b"]);
        assert_eq!(p.label_changed, ["c", "d"]);
        assert_eq!(p.next_page_token.as_deref(), Some("n"));
        assert_eq!(p.history_id, "99");
        assert!(to_history_page(gm::HistoryList::default()).is_err());
    }

    #[test]
    fn maps_labels() {
        let l: gm::LabelList = serde_json::from_value(json!({"labels": [
            {"id": "INBOX", "name": "INBOX", "type": "system"},
            {"id": "L1", "name": "Work", "type": "user"}
        ]}))
        .unwrap();
        let v: Vec<_> = l.labels.into_iter().map(to_label).collect();
        assert!(v[0].is_system && !v[1].is_system);
    }

    #[test]
    fn maps_errors() {
        assert!(matches!(map_err(gm::Error::AuthExpired), ApiError::Auth(_)));
        assert!(matches!(
            map_err(gm::Error::Auth("x".into())),
            ApiError::Auth(_)
        ));
        assert!(matches!(
            map_err(gm::Error::HistoryExpired),
            ApiError::HistoryExpired
        ));
        assert!(matches!(
            map_err(gm::Error::RateLimited),
            ApiError::RateLimited
        ));
        assert!(matches!(
            map_err(gm::Error::Config("x".into())),
            ApiError::Setup(_)
        ));
        let api = gm::Error::Api {
            status: 500,
            reason: String::new(),
            message: "boom".into(),
        };
        assert!(matches!(map_err(api), ApiError::Other(_)));
    }
}
