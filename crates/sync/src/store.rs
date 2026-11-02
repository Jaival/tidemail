//! SQL for the sync engine: ingest remote messages, delete, local action
//! application, snapshots. Move to `tidemail_db` on merge if shared.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tidemail_db::rusqlite::{params, Connection, OptionalExtension};
use tidemail_db::types::BodyState;
use tidemail_db::write::{self, NewAttachment, NewMessage};
use tidemail_render::{html_to_text, sanitize};
use tidemail_search::fts::{self, FtsRow};

use crate::api::{RemoteLabel, RemoteMessage};
use crate::{Error, Result};

/// Inline `cid:` parts below this size are stored in `inline_blobs`.
pub const INLINE_MAX: u32 = 200 * 1024;

const SYSTEM_IDS: &[&str] = &[
    "INBOX",
    "SENT",
    "DRAFT",
    "STARRED",
    "UNREAD",
    "IMPORTANT",
    "TRASH",
    "SPAM",
    "CHAT",
    "CATEGORY_PERSONAL",
    "CATEGORY_SOCIAL",
    "CATEGORY_PROMOTIONS",
    "CATEGORY_UPDATES",
    "CATEGORY_FORUMS",
];

/// Gmail label id -> (label row id, label_key).
pub type LabelMap = HashMap<String, (i64, String)>;

pub fn label_map(conn: &Connection, account_id: i64) -> Result<LabelMap> {
    let mut st =
        conn.prepare_cached("SELECT gmail_id, id, label_key FROM labels WHERE account_id = ?1")?;
    let rows = st.query_map([account_id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn sync_labels(conn: &Connection, account_id: i64, labels: &[RemoteLabel]) -> Result<()> {
    for l in labels {
        write::upsert_label(
            conn,
            account_id,
            &l.id,
            &l.name,
            l.is_system,
            l.color.as_deref(),
        )?;
    }
    Ok(())
}

/// Row id of a label; creates known system labels that are missing.
fn label_row(conn: &Connection, account_id: i64, gmail_id: &str) -> Result<i64> {
    let found = conn
        .prepare_cached("SELECT id FROM labels WHERE account_id = ?1 AND gmail_id = ?2")?
        .query_row(params![account_id, gmail_id], |r| r.get(0))
        .optional()?;
    match found {
        Some(id) => Ok(id),
        None if SYSTEM_IDS.contains(&gmail_id) => Ok(write::upsert_label(
            conn, account_id, gmail_id, gmail_id, true, None,
        )?),
        None => Err(Error::NotFound(format!("label {gmail_id}"))),
    }
}

/// Gmail label id for a `label_key` of this account.
pub fn gmail_label_id(conn: &Connection, account_id: i64, label_key: &str) -> Result<String> {
    conn.prepare_cached("SELECT gmail_id FROM labels WHERE account_id = ?1 AND label_key = ?2")?
        .query_row(params![account_id, label_key], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("label {label_key}")))
}

/// `"Name" <a@b.c>` -> (name, email).
pub fn parse_address(raw: &str) -> (String, String) {
    let raw = raw.trim();
    match (raw.rfind('<'), raw.rfind('>')) {
        (Some(a), Some(b)) if a < b => (
            raw[..a].trim().trim_matches('"').trim().to_owned(),
            raw[a + 1..b].trim().to_owned(),
        ),
        _ => (String::new(), raw.to_owned()),
    }
}

/// Insert or update a message from metadata. Does not recompute the thread.
/// Returns the local thread id.
pub fn ingest_meta(
    conn: &Connection,
    account_id: i64,
    labels: &LabelMap,
    m: &RemoteMessage,
) -> Result<i64> {
    let thread_id = write::ensure_thread(conn, account_id, &m.thread_id)?;
    let ids: Vec<i64> = m
        .label_ids
        .iter()
        .filter_map(|g| labels.get(g).map(|(id, _)| *id))
        .collect();
    let unread = m.label_ids.iter().any(|l| l == "UNREAD");
    let starred = m.label_ids.iter().any(|l| l == "STARRED");

    let existing: Option<i64> = conn
        .prepare_cached("SELECT id FROM messages WHERE account_id = ?1 AND gmail_id = ?2")?
        .query_row(params![account_id, m.id], |r| r.get(0))
        .optional()?;
    let message_id = if let Some(id) = existing {
        conn.prepare_cached("UPDATE messages SET is_unread = ?2, is_starred = ?3 WHERE id = ?1")?
            .execute(params![id, unread, starred])?;
        id
    } else {
        let (from_name, from_email) = parse_address(&m.from);
        let to_text = [m.to.as_str(), m.cc.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let id = write::insert_message(
            conn,
            &NewMessage {
                account_id,
                gmail_id: &m.id,
                thread_id,
                from_name: &from_name,
                from_email: &from_email,
                to_text: &to_text,
                subject: &m.subject,
                snippet: &m.snippet,
                internal_date: m.internal_date_ms,
                is_unread: unread,
                is_starred: starred,
                has_attachment: m.has_attachment,
                body_state: BodyState::Missing,
            },
        )?;
        fts::insert(
            conn,
            id,
            &FtsRow {
                from_name: &from_name,
                from_email: &from_email.to_ascii_lowercase(),
                to_text: &to_text,
                subject: &m.subject,
                body_text: "",
            },
        )?;
        id
    };
    write::set_message_labels(conn, message_id, &ids)?;
    Ok(thread_id)
}

/// Store the body of a message fetched with `Format::Full`.
/// `blobs` maps `part_id` to inline bytes (already fetched).
pub fn ingest_full(
    conn: &Connection,
    message_id: i64,
    m: &RemoteMessage,
    blobs: &HashMap<String, Vec<u8>>,
) -> Result<()> {
    let body = m
        .body
        .as_ref()
        .ok_or_else(|| Error::NotFound("body".into()))?;
    let html = body
        .html
        .as_deref()
        .filter(|h| !h.trim().is_empty())
        .map(|h| sanitize(h, message_id));
    let text = match (&body.text, &body.html) {
        (Some(t), _) if !t.trim().is_empty() => t.clone(),
        (_, Some(h)) => html_to_text(h),
        _ => String::new(),
    };

    let (from_name, from_email, to_text, subject): (String, String, String, String) = conn
        .prepare_cached(
            "SELECT from_name, from_email, to_text, subject FROM messages WHERE id = ?1",
        )?
        .query_row([message_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
    let old_text: String = conn
        .prepare_cached("SELECT text FROM bodies WHERE message_id = ?1")?
        .query_row([message_id], |r| r.get(0))
        .optional()?
        .unwrap_or_default();
    let old = FtsRow {
        from_name: &from_name,
        from_email: &from_email,
        to_text: &to_text,
        subject: &subject,
        body_text: &old_text,
    };
    write::upsert_body(
        conn,
        message_id,
        html.as_deref(),
        &text,
        tidemail_render::SANITIZER_VERSION,
    )?;
    fts::replace_body(conn, message_id, &old, &text)?;

    conn.prepare_cached("DELETE FROM attachments WHERE message_id = ?1")?
        .execute([message_id])?;
    for a in &body.attachments {
        let id = write::insert_attachment(
            conn,
            &NewAttachment {
                message_id,
                part_id: &a.part_id,
                gmail_attachment_id: a.attachment_id.as_deref(),
                filename: &a.filename,
                mime_type: &a.mime_type,
                size: a.size,
                content_id: a.content_id.as_deref(),
                is_inline: a.is_inline,
            },
        )?;
        if let Some(data) = blobs.get(&a.part_id) {
            write::insert_inline_blob(conn, id, data)?;
        }
    }
    let has_att = body.attachments.iter().any(|a| !a.is_inline);
    conn.prepare_cached("UPDATE messages SET has_attachment = ?2 WHERE id = ?1")?
        .execute(params![message_id, has_att])?;
    Ok(())
}

type DeleteRow = (i64, i64, String, String, String, String, String);

/// Delete a message (and its FTS row). Returns its local thread id.
pub fn delete_message(conn: &Connection, account_id: i64, gmail_id: &str) -> Result<Option<i64>> {
    let row: Option<DeleteRow> = conn
        .prepare_cached(
            "SELECT m.id, m.thread_id, m.from_name, m.from_email, m.to_text, m.subject, \
             COALESCE(b.text, '') FROM messages m LEFT JOIN bodies b ON b.message_id = m.id \
             WHERE m.account_id = ?1 AND m.gmail_id = ?2",
        )?
        .query_row(params![account_id, gmail_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .optional()?;
    let Some((id, thread_id, from_name, from_email, to_text, subject, body_text)) = row else {
        return Ok(None);
    };
    fts::delete(
        conn,
        id,
        &FtsRow {
            from_name: &from_name,
            from_email: &from_email,
            to_text: &to_text,
            subject: &subject,
            body_text: &body_text,
        },
    )?;
    conn.prepare_cached("DELETE FROM messages WHERE id = ?1")?
        .execute([id])?;
    Ok(Some(thread_id))
}

/// Delete every message of an account from FTS (before the account row goes).
pub fn delete_account_fts(conn: &Connection, account_id: i64) -> Result<()> {
    let ids: Vec<String> = conn
        .prepare_cached("SELECT gmail_id FROM messages WHERE account_id = ?1")?
        .query_map([account_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    for g in ids {
        delete_message(conn, account_id, &g)?;
    }
    Ok(())
}

pub fn thread_label_keys(conn: &Connection, thread_id: i64) -> Result<Vec<String>> {
    Ok(conn
        .prepare_cached("SELECT label_key FROM thread_labels WHERE thread_id = ?1")?
        .query_map([thread_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?)
}

// ---- local actions ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Remote {
    Modify,
    Trash,
    Untrash,
}

/// What an action does to Gmail labels (Gmail label ids).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub remote: Remote,
}

impl Op {
    pub fn inverse(&self) -> Op {
        Op {
            add: self.remove.clone(),
            remove: self.add.clone(),
            remote: match self.remote {
                Remote::Modify => Remote::Modify,
                Remote::Trash => Remote::Untrash,
                Remote::Untrash => Remote::Trash,
            },
        }
    }
}

/// Label set of one message before an action (for exact local undo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgState {
    pub id: i64,
    pub labels: Vec<i64>,
}

pub fn snapshot(conn: &Connection, thread_id: i64) -> Result<Vec<MsgState>> {
    let mut st = conn.prepare_cached(
        "SELECT m.id, ml.label_id FROM messages m \
         LEFT JOIN message_labels ml ON ml.message_id = m.id WHERE m.thread_id = ?1 ORDER BY m.id",
    )?;
    let mut out: Vec<MsgState> = Vec::new();
    let mut rows = st.query([thread_id])?;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let label: Option<i64> = r.get(1)?;
        if out.last().map(|m| m.id) != Some(id) {
            out.push(MsgState { id, labels: vec![] });
        }
        if let (Some(l), Some(m)) = (label, out.last_mut()) {
            m.labels.push(l);
        }
    }
    Ok(out)
}

fn sync_flags(conn: &Connection, account_id: i64, thread_id: i64) -> Result<()> {
    let unread = label_row(conn, account_id, "UNREAD")?;
    let starred = label_row(conn, account_id, "STARRED")?;
    conn.prepare_cached(
        "UPDATE messages SET \
         is_unread = EXISTS (SELECT 1 FROM message_labels WHERE message_id = messages.id AND label_id = ?2), \
         is_starred = EXISTS (SELECT 1 FROM message_labels WHERE message_id = messages.id AND label_id = ?3) \
         WHERE thread_id = ?1",
    )?
    .execute(params![thread_id, unread, starred])?;
    Ok(())
}

/// Apply an op to every message of a thread, then recompute the thread.
pub fn apply_op(conn: &Connection, account_id: i64, thread_id: i64, op: &Op) -> Result<()> {
    for g in &op.add {
        let l = label_row(conn, account_id, g)?;
        conn.prepare_cached(
            "INSERT OR IGNORE INTO message_labels (message_id, label_id) \
             SELECT id, ?2 FROM messages WHERE thread_id = ?1",
        )?
        .execute(params![thread_id, l])?;
    }
    for g in &op.remove {
        let l = label_row(conn, account_id, g)?;
        conn.prepare_cached(
            "DELETE FROM message_labels WHERE label_id = ?2 \
             AND message_id IN (SELECT id FROM messages WHERE thread_id = ?1)",
        )?
        .execute(params![thread_id, l])?;
    }
    sync_flags(conn, account_id, thread_id)?;
    write::recompute_thread(conn, thread_id)?;
    Ok(())
}

/// Put messages back to a snapshot (in-window undo).
pub fn restore_snapshot(
    conn: &Connection,
    account_id: i64,
    thread_id: i64,
    before: &[MsgState],
) -> Result<()> {
    for m in before {
        write::set_message_labels(conn, m.id, &m.labels)?;
    }
    sync_flags(conn, account_id, thread_id)?;
    write::recompute_thread(conn, thread_id)?;
    Ok(())
}
