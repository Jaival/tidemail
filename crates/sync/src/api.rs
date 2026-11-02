//! The seam between the sync engine and Gmail.
//!
//! The engine only talks to these traits. The app crate provides an adapter
//! from `tidemail_gmail` (REST client, OAuth) to them. Tests use `fake.rs`.
//!
//! Contract for implementers:
//! - Retry 429 / 403 `rateLimitExceeded` inside the adapter (quota is 250
//!   units/user/s) and only return `ApiError::RateLimited` when giving up.
//! - Return `ApiError::Offline` for transport errors, `ApiError::Auth` for
//!   401 / revoked tokens, `ApiError::HistoryExpired` for 404 on `history.list`.
//! - `get_messages` takes at most 50 ids (one batch request). Ids that Gmail
//!   reports as 404 are left out of the result; any other per-id failure fails
//!   the whole call.

use std::sync::Arc;

use async_trait::async_trait;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ApiError {
    #[error("offline: {0}")]
    Offline(String),
    #[error("authorization failed: {0}")]
    Auth(String),
    #[error("history expired")]
    HistoryExpired,
    #[error("rate limited")]
    RateLimited,
    #[error("{0}")]
    Other(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone)]
pub struct Profile {
    pub email: String,
    /// Current mailbox history id; start point for `list_history`.
    pub history_id: String,
}

#[derive(Debug, Clone)]
pub struct MessageRef {
    pub id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct IdPage {
    /// Newest first.
    pub ids: Vec<MessageRef>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Headers, labels, snippet, date.
    Metadata,
    /// Metadata plus body parts and attachment descriptors.
    Full,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteAttachment {
    pub part_id: String,
    /// `None` when the part data is inline in the message (small parts).
    pub attachment_id: Option<String>,
    pub filename: String,
    pub mime_type: String,
    pub size: u32,
    /// Without angle brackets.
    pub content_id: Option<String>,
    pub is_inline: bool,
    /// Part bytes when Gmail returned them inline.
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteBody {
    /// Raw HTML (not sanitized).
    pub html: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<RemoteAttachment>,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteMessage {
    pub id: String,
    pub thread_id: String,
    /// Gmail label ids, including `UNREAD` and `STARRED`.
    pub label_ids: Vec<String>,
    pub snippet: String,
    pub internal_date_ms: i64,
    /// Raw header values (`"Name" <a@b.c>`).
    pub from: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    /// Best effort in `Metadata` format (the engine corrects it from `Full`).
    pub has_attachment: bool,
    /// `Some` only for `Format::Full`.
    pub body: Option<RemoteBody>,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryPage {
    /// Ids from `messagesAdded`.
    pub added: Vec<String>,
    /// Ids from `messagesDeleted`.
    pub deleted: Vec<String>,
    /// Ids from `labelsAdded` / `labelsRemoved`.
    pub label_changed: Vec<String>,
    pub next_page_token: Option<String>,
    /// Mailbox history id as of this response.
    pub history_id: String,
}

#[derive(Debug, Clone)]
pub struct RemoteLabel {
    pub id: String,
    pub name: String,
    pub is_system: bool,
    pub color: Option<String>,
}

#[async_trait]
pub trait GmailApi: Send + Sync {
    async fn get_profile(&self) -> ApiResult<Profile>;
    /// `messages.list`; `label` is a label id filter.
    async fn list_message_ids(
        &self,
        label: Option<&str>,
        page_token: Option<&str>,
    ) -> ApiResult<IdPage>;
    async fn get_messages(&self, ids: &[String], format: Format) -> ApiResult<Vec<RemoteMessage>>;
    async fn get_attachment(&self, message_id: &str, attachment_id: &str) -> ApiResult<Vec<u8>>;
    async fn list_history(
        &self,
        start_history_id: &str,
        page_token: Option<&str>,
    ) -> ApiResult<HistoryPage>;
    async fn list_labels(&self) -> ApiResult<Vec<RemoteLabel>>;
    async fn threads_modify(
        &self,
        thread_id: &str,
        add_labels: &[String],
        remove_labels: &[String],
    ) -> ApiResult<()>;
    async fn threads_trash(&self, thread_id: &str) -> ApiResult<()>;
    async fn threads_untrash(&self, thread_id: &str) -> ApiResult<()>;
}

#[derive(Debug, Clone)]
pub struct SignedInAccount {
    pub email: String,
    pub display_name: Option<String>,
}

/// Sign-in and per-account API access, injected into the engine.
#[async_trait]
pub trait GmailProvider: Send + Sync + 'static {
    /// Browser sign-in; stores the refresh token.
    async fn sign_in(&self) -> ApiResult<SignedInAccount>;
    /// API handle for an already signed-in account.
    fn api(&self, email: &str) -> ApiResult<Arc<dyn GmailApi>>;
    /// Forget the stored token.
    fn sign_out(&self, email: &str) -> ApiResult<()>;
}
