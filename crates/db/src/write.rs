//! Write helpers shared by the sync engine and the fake mailbox generator.
//! FTS rows are not touched here; see `tidemail-search::fts`.

use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::types::BodyState;

pub struct NewAccount<'a> {
    pub email: &'a str,
    pub display_name: Option<&'a str>,
    pub color: &'a str,
    pub sort_order: i64,
}

pub fn insert_account(conn: &Connection, a: &NewAccount) -> Result<i64> {
    conn.prepare_cached(
        "INSERT INTO accounts (email, display_name, color, sort_order) VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![a.email, a.display_name, a.color, a.sort_order])?;
    Ok(conn.last_insert_rowid())
}

/// Key used in `thread_labels`: system labels are shared across accounts.
pub fn label_key(account_id: i64, gmail_id: &str, is_system: bool) -> String {
    if is_system {
        gmail_id.to_owned()
    } else {
        format!("u:{account_id}:{gmail_id}")
    }
}

pub fn upsert_label(
    conn: &Connection,
    account_id: i64,
    gmail_id: &str,
    name: &str,
    is_system: bool,
    color: Option<&str>,
) -> Result<i64> {
    let key = label_key(account_id, gmail_id, is_system);
    conn.prepare_cached(
        "INSERT INTO labels (account_id, gmail_id, label_key, name, is_system, color) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (account_id, gmail_id) DO UPDATE SET name = excluded.name, color = excluded.color \
         RETURNING id",
    )?
    .query_row(params![account_id, gmail_id, key, name, is_system, color], |r| r.get(0))
}

/// Id of the thread row for a Gmail thread id, creating an empty shell if
/// needed. Call `recompute_thread` after its messages change.
pub fn ensure_thread(conn: &Connection, account_id: i64, gmail_thread_id: &str) -> Result<i64> {
    if let Some(id) = conn
        .prepare_cached("SELECT id FROM threads WHERE account_id = ?1 AND gmail_id = ?2")?
        .query_row(params![account_id, gmail_thread_id], |r| r.get(0))
        .optional()?
    {
        return Ok(id);
    }
    conn.prepare_cached(
        "INSERT INTO threads (account_id, gmail_id, last_date, subject, snippet, participants, \
         message_count, unread_count, has_attachment, is_starred) \
         VALUES (?1, ?2, 0, '', '', '', 0, 0, 0, 0)",
    )?
    .execute(params![account_id, gmail_thread_id])?;
    Ok(conn.last_insert_rowid())
}

pub struct NewMessage<'a> {
    pub account_id: i64,
    pub gmail_id: &'a str,
    pub thread_id: i64,
    pub from_name: &'a str,
    pub from_email: &'a str,
    pub to_text: &'a str,
    pub subject: &'a str,
    pub snippet: &'a str,
    pub internal_date: i64,
    pub is_unread: bool,
    pub is_starred: bool,
    pub has_attachment: bool,
    pub body_state: BodyState,
}

/// Stores `from_email` lowercased, so `sender_prefs` lookups match.
/// Pass the lowercased address to `tidemail_search::fts` as well.
pub fn insert_message(conn: &Connection, m: &NewMessage) -> Result<i64> {
    conn.prepare_cached(
        "INSERT INTO messages (account_id, gmail_id, thread_id, from_name, from_email, to_text, \
         subject, snippet, internal_date, is_unread, is_starred, has_attachment, body_state) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?
    .execute(params![
        m.account_id,
        m.gmail_id,
        m.thread_id,
        m.from_name,
        m.from_email.to_ascii_lowercase(),
        m.to_text,
        m.subject,
        m.snippet,
        m.internal_date,
        m.is_unread,
        m.is_starred,
        m.has_attachment,
        m.body_state.to_db(),
    ])?;
    Ok(conn.last_insert_rowid())
}

/// Replace the label set of a message. Call `recompute_thread` afterwards.
pub fn set_message_labels(conn: &Connection, message_id: i64, label_ids: &[i64]) -> Result<()> {
    conn.prepare_cached("DELETE FROM message_labels WHERE message_id = ?1")?
        .execute([message_id])?;
    let mut st = conn.prepare_cached(
        "INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?1, ?2)",
    )?;
    for l in label_ids {
        st.execute([message_id, *l])?;
    }
    Ok(())
}

pub fn upsert_body(
    conn: &Connection,
    message_id: i64,
    html_sanitized: Option<&str>,
    text: &str,
    sanitizer_version: u32,
) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO bodies (message_id, html_sanitized, text, sanitizer_version) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT (message_id) DO UPDATE SET html_sanitized = excluded.html_sanitized, \
         text = excluded.text, sanitizer_version = excluded.sanitizer_version",
    )?
    .execute(params![message_id, html_sanitized, text, sanitizer_version])?;
    conn.prepare_cached("UPDATE messages SET body_state = 1 WHERE id = ?1")?
        .execute([message_id])?;
    Ok(())
}

pub struct NewAttachment<'a> {
    pub message_id: i64,
    pub part_id: &'a str,
    pub gmail_attachment_id: Option<&'a str>,
    pub filename: &'a str,
    pub mime_type: &'a str,
    pub size: u32,
    pub content_id: Option<&'a str>,
    pub is_inline: bool,
}

pub fn insert_attachment(conn: &Connection, a: &NewAttachment) -> Result<i64> {
    conn.prepare_cached(
        "INSERT INTO attachments (message_id, part_id, gmail_attachment_id, filename, mime_type, \
         size, content_id, is_inline) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?
    .execute(params![
        a.message_id,
        a.part_id,
        a.gmail_attachment_id,
        a.filename,
        a.mime_type,
        a.size,
        a.content_id,
        a.is_inline
    ])?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_inline_blob(conn: &Connection, attachment_id: i64, data: &[u8]) -> Result<()> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO inline_blobs (attachment_id, data) VALUES (?1, ?2)",
    )?
    .execute(params![attachment_id, data])?;
    Ok(())
}

pub fn set_sender_images(conn: &Connection, email: &str, load_images: bool) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO sender_prefs (email, load_images) VALUES (?1, ?2) \
         ON CONFLICT (email) DO UPDATE SET load_images = excluded.load_images",
    )?
    .execute(params![email.to_ascii_lowercase(), load_images])?;
    Ok(())
}

/// Rebuild the denormalized `threads` row and its `thread_labels` from the
/// thread's messages. Deletes the thread when it has no messages left.
/// Call after every message insert, delete or label change.
pub fn recompute_thread(conn: &Connection, thread_id: i64) -> Result<()> {
    let (count, unread, has_att, starred, last): (i64, i64, bool, bool, Option<i64>) = conn
        .prepare_cached(
            "SELECT count(*), COALESCE(sum(is_unread), 0), COALESCE(max(has_attachment), 0), \
             COALESCE(max(is_starred), 0), max(internal_date) FROM messages WHERE thread_id = ?1",
        )?
        .query_row([thread_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
    if count == 0 {
        conn.prepare_cached("DELETE FROM threads WHERE id = ?1")?
            .execute([thread_id])?;
        return Ok(());
    }
    let last = last.unwrap_or(0);

    let mut subject = String::new();
    let mut snippet = String::new();
    let mut names: Vec<String> = Vec::new();
    {
        let mut st = conn.prepare_cached(
            "SELECT subject, snippet, from_name, from_email FROM messages \
             WHERE thread_id = ?1 ORDER BY internal_date, id",
        )?;
        let mut rows = st.query([thread_id])?;
        let mut first = true;
        while let Some(r) = rows.next()? {
            let s: String = r.get(0)?;
            if first {
                subject = s;
                first = false;
            }
            snippet = r.get(1)?;
            let name: String = r.get(2)?;
            let name = if name.is_empty() { r.get(3)? } else { name };
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let participants = if names.len() > 4 {
        format!("{} … {}", names[0], names[names.len() - 2..].join(", "))
    } else {
        names.join(", ")
    };

    let account_id: i64 = conn
        .prepare_cached(
            "UPDATE threads SET last_date = ?2, subject = ?3, snippet = ?4, participants = ?5, \
             message_count = ?6, unread_count = ?7, has_attachment = ?8, is_starred = ?9 \
             WHERE id = ?1 RETURNING account_id",
        )?
        .query_row(
            params![
                thread_id,
                last,
                subject,
                snippet,
                participants,
                count,
                unread,
                has_att,
                starred
            ],
            |r| r.get(0),
        )?;

    conn.prepare_cached("DELETE FROM thread_labels WHERE thread_id = ?1")?
        .execute([thread_id])?;
    conn.prepare_cached(
        "INSERT INTO thread_labels (label_key, last_date, thread_id, account_id) \
         SELECT DISTINCT l.label_key, ?2, ?1, ?3 \
         FROM messages m JOIN message_labels ml ON ml.message_id = m.id \
         JOIN labels l ON l.id = ml.label_id WHERE m.thread_id = ?1",
    )?
    .execute(params![thread_id, last, account_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read;
    use crate::types::View;

    #[test]
    fn recompute_builds_thread_and_labels() {
        let conn = crate::open_in_memory().unwrap();
        let acct = insert_account(
            &conn,
            &NewAccount {
                email: "a@x.com",
                display_name: None,
                color: "#f00",
                sort_order: 0,
            },
        )
        .unwrap();
        let inbox = upsert_label(&conn, acct, "INBOX", "Inbox", true, None).unwrap();
        let work = upsert_label(&conn, acct, "Label_1", "Work", false, None).unwrap();
        let t = ensure_thread(&conn, acct, "t1").unwrap();
        for (i, labels) in [vec![inbox], vec![inbox, work]].iter().enumerate() {
            let gid = format!("m{i}");
            let m = insert_message(
                &conn,
                &NewMessage {
                    account_id: acct,
                    gmail_id: &gid,
                    thread_id: t,
                    from_name: if i == 0 { "Ann" } else { "Bob" },
                    from_email: "x@y.z",
                    to_text: "",
                    subject: "Hello",
                    snippet: &format!("snip {i}"),
                    internal_date: 1000 + i as i64,
                    is_unread: i == 1,
                    is_starred: false,
                    has_attachment: false,
                    body_state: BodyState::Missing,
                },
            )
            .unwrap();
            set_message_labels(&conn, m, labels).unwrap();
        }
        recompute_thread(&conn, t).unwrap();

        let page = read::list_threads(
            &conn,
            &View {
                label_key: "INBOX".into(),
                account_id: None,
            },
            None,
            50,
        )
        .unwrap();
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.message_count, 2);
        assert_eq!(row.unread_count, 1);
        assert_eq!(row.last_date, 1001);
        assert_eq!(row.snippet, "snip 1");
        assert_eq!(row.participants, "Ann, Bob");
        assert!(page.next.is_none());
        // A full last page has no next cursor (no extra empty fetch).
        let exact = read::list_threads(
            &conn,
            &View {
                label_key: "INBOX".into(),
                account_id: None,
            },
            None,
            1,
        )
        .unwrap();
        assert_eq!(exact.items.len(), 1);
        assert!(exact.next.is_none());

        let per_acct = read::list_threads(
            &conn,
            &View {
                label_key: format!("u:{acct}:Label_1"),
                account_id: Some(acct),
            },
            None,
            50,
        )
        .unwrap();
        assert_eq!(per_acct.items.len(), 1);
    }
}
