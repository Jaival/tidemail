//! Types that cross the IPC boundary. `tauri-specta` exports them to
//! `src/bindings.ts`. Frozen for Phase 1; changes go through docs/HANDOFF.md.
//!
//! Ids are SQLite rowids (`i64`, exported as TS `number`). Timestamps are ms.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: i64,
    pub email: String,
    pub display_name: Option<String>,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: i64,
    pub account_id: i64,
    pub gmail_id: String,
    /// System labels share a key across accounts ("INBOX"); user labels are
    /// `u:<account_id>:<gmail_id>`.
    pub label_key: String,
    pub name: String,
    pub is_system: bool,
    pub color: Option<String>,
}

/// A list: a label, either unified (all accounts) or for one account.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub label_key: String,
    pub account_id: Option<i64>,
}

/// Keyset paging cursor: the last row of the previous page.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub last_date: i64,
    pub thread_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRow {
    pub id: i64,
    pub account_id: i64,
    pub last_date: i64,
    pub subject: String,
    pub snippet: String,
    pub participants: String,
    pub message_count: u32,
    pub unread_count: u32,
    pub has_attachment: bool,
    pub is_starred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPage {
    pub items: Vec<ThreadRow>,
    /// `None` when this is the last page.
    pub next: Option<Cursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BodyState {
    Missing,
    Stored,
    Deferred,
}

impl BodyState {
    pub fn from_db(v: i64) -> Self {
        match v {
            1 => Self::Stored,
            2 => Self::Deferred,
            _ => Self::Missing,
        }
    }
    pub fn to_db(self) -> i64 {
        match self {
            Self::Missing => 0,
            Self::Stored => 1,
            Self::Deferred => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: i64,
    pub message_id: i64,
    pub filename: String,
    pub mime_type: String,
    pub size: u32,
    pub content_id: Option<String>,
    pub is_inline: bool,
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageMeta {
    pub id: i64,
    pub from_name: String,
    pub from_email: String,
    pub to_text: String,
    pub subject: String,
    pub snippet: String,
    pub internal_date: i64,
    pub is_unread: bool,
    pub is_starred: bool,
    pub has_attachment: bool,
    pub body_state: BodyState,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDetail {
    pub thread: ThreadRow,
    /// Oldest first.
    pub messages: Vec<MessageMeta>,
    pub label_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub message_id: i64,
    /// Sanitized at ingest. Never sanitized again at read time.
    pub html: Option<String>,
    pub text: String,
    pub sanitizer_version: u32,
    /// Sender is allowed to load remote images (`sender_prefs`).
    pub load_images: bool,
}

// ---- Phase 1 types (commands are stubs in Phase 0) ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ActionKind {
    Archive,
    Unarchive,
    Trash,
    Untrash,
    Star,
    Unstar,
    MarkRead,
    MarkUnread,
    AddLabel {
        label_key: String,
    },
    RemoveLabel {
        label_key: String,
    },
    /// Add `label_key`, remove INBOX, in one `threads.modify`.
    Move {
        label_key: String,
    },
}

/// Applies to whole threads.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    pub thread_ids: Vec<i64>,
    pub kind: ActionKind,
}

/// `action_queue` row ids created by one request. Pass to `undo_action`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActionTicket {
    pub action_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub thread: ThreadRow,
    /// Best-matching message in the thread.
    pub message_id: i64,
    /// Snippet with matches wrapped in `<mark>`; all other text HTML-escaped.
    pub snippet_html: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AccountSyncState {
    Idle,
    Syncing,
    Offline,
    AuthError,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountSync {
    pub account_id: i64,
    pub state: AccountSyncState,
    pub last_sync_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FailedAction {
    pub id: i64,
    pub thread_id: i64,
    pub kind: ActionKind,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub accounts: Vec<AccountSync>,
    pub pending_actions: u32,
    pub failed_actions: Vec<FailedAction>,
}
