//! Typed Gmail API responses (only the fields sync needs).

use serde::{Deserialize, Deserializer};

fn ms_from_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|s| s.parse().map_err(serde::de::Error::custom))
        .transpose()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub email_address: String,
    pub history_id: String,
    #[serde(default)]
    pub messages_total: Option<i64>,
    #[serde(default)]
    pub threads_total: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRef {
    pub id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageList {
    #[serde(default)]
    pub messages: Vec<MessageRef>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartBody {
    pub attachment_id: Option<String>,
    #[serde(default)]
    pub size: u64,
    /// base64url
    pub data: Option<String>,
}

/// One MIME part. The root part is `Message::payload`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub part_id: Option<String>,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub body: PartBody,
    #[serde(default)]
    pub parts: Vec<Payload>,
}

impl Payload {
    /// Case-insensitive header lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    #[serde(default)]
    pub label_ids: Vec<String>,
    #[serde(default)]
    pub snippet: String,
    pub history_id: Option<String>,
    #[serde(default, deserialize_with = "ms_from_string")]
    pub internal_date: Option<i64>,
    #[serde(default)]
    pub size_estimate: u64,
    pub payload: Option<Payload>,
}

impl Message {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.payload.as_ref()?.header(name)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttachmentBody {
    #[serde(default)]
    pub size: u64,
    /// base64url
    pub data: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAdded {
    pub message: Message,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelChange {
    pub message: Message,
    #[serde(default)]
    pub label_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: String,
    #[serde(default)]
    pub messages_added: Vec<MessageAdded>,
    #[serde(default)]
    pub messages_deleted: Vec<MessageAdded>,
    #[serde(default)]
    pub labels_added: Vec<LabelChange>,
    #[serde(default)]
    pub labels_removed: Vec<LabelChange>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryList {
    #[serde(default)]
    pub history: Vec<HistoryRecord>,
    pub next_page_token: Option<String>,
    pub history_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    /// `system` or `user`.
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LabelList {
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// Response of `threads.modify` / `trash` / `untrash`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub history_id: Option<String>,
}
