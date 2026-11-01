//! Read queries used by the UI. All use cached prepared statements and an
//! index range scan (checked by tests with EXPLAIN QUERY PLAN).

use rusqlite::{params, Connection, OptionalExtension, Result, Row};

use crate::types::*;

/// Hard cap on rows per IPC call.
pub const MAX_PAGE: u32 = 100;

const THREAD_COLS: &str = "t.id, t.account_id, t.last_date, t.subject, t.snippet, t.participants, \
     t.message_count, t.unread_count, t.has_attachment, t.is_starred";

pub const SQL_LIST_UNIFIED: &str = "SELECT t.id, t.account_id, t.last_date, t.subject, t.snippet, \
     t.participants, t.message_count, t.unread_count, t.has_attachment, t.is_starred \
     FROM thread_labels tl JOIN threads t ON t.id = tl.thread_id \
     WHERE tl.label_key = ?1 AND (tl.last_date, tl.thread_id) < (?2, ?3) \
     ORDER BY tl.last_date DESC, tl.thread_id DESC LIMIT ?4";

pub const SQL_LIST_ACCOUNT: &str = "SELECT t.id, t.account_id, t.last_date, t.subject, t.snippet, \
     t.participants, t.message_count, t.unread_count, t.has_attachment, t.is_starred \
     FROM thread_labels tl JOIN threads t ON t.id = tl.thread_id \
     WHERE tl.account_id = ?5 AND tl.label_key = ?1 AND (tl.last_date, tl.thread_id) < (?2, ?3) \
     ORDER BY tl.last_date DESC, tl.thread_id DESC LIMIT ?4";

pub fn thread_row(r: &Row) -> Result<ThreadRow> {
    Ok(ThreadRow {
        id: r.get(0)?,
        account_id: r.get(1)?,
        last_date: r.get(2)?,
        subject: r.get(3)?,
        snippet: r.get(4)?,
        participants: r.get(5)?,
        message_count: r.get(6)?,
        unread_count: r.get(7)?,
        has_attachment: r.get(8)?,
        is_starred: r.get(9)?,
    })
}

pub fn list_accounts(conn: &Connection) -> Result<Vec<Account>> {
    let mut st = conn.prepare_cached(
        "SELECT id, email, display_name, color FROM accounts ORDER BY sort_order, id",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Account {
            id: r.get(0)?,
            email: r.get(1)?,
            display_name: r.get(2)?,
            color: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// Labels of one account, or of all accounts when `account_id` is `None`.
pub fn list_labels(conn: &Connection, account_id: Option<i64>) -> Result<Vec<Label>> {
    let mut st = conn.prepare_cached(
        "SELECT id, account_id, gmail_id, label_key, name, is_system, color FROM labels \
         WHERE ?1 IS NULL OR account_id = ?1 ORDER BY account_id, is_system DESC, name",
    )?;
    let rows = st.query_map([account_id], |r| {
        Ok(Label {
            id: r.get(0)?,
            account_id: r.get(1)?,
            gmail_id: r.get(2)?,
            label_key: r.get(3)?,
            name: r.get(4)?,
            is_system: r.get(5)?,
            color: r.get(6)?,
        })
    })?;
    rows.collect()
}

/// One page of threads in a view, newest first.
pub fn list_threads(
    conn: &Connection,
    view: &View,
    cursor: Option<Cursor>,
    limit: u32,
) -> Result<ThreadPage> {
    let limit = limit.clamp(1, MAX_PAGE);
    let (date, id) = cursor.map_or((i64::MAX, i64::MAX), |c| (c.last_date, c.thread_id));
    let items: Vec<ThreadRow> = match view.account_id {
        None => {
            let mut st = conn.prepare_cached(SQL_LIST_UNIFIED)?;
            let rows = st.query_map(params![view.label_key, date, id, limit], thread_row)?;
            rows.collect::<Result<_>>()?
        }
        Some(acct) => {
            let mut st = conn.prepare_cached(SQL_LIST_ACCOUNT)?;
            let rows = st.query_map(params![view.label_key, date, id, limit, acct], thread_row)?;
            rows.collect::<Result<_>>()?
        }
    };
    let next = (items.len() as u32 == limit).then(|| {
        let last = items.last().expect("non-empty page");
        Cursor {
            last_date: last.last_date,
            thread_id: last.id,
        }
    });
    Ok(ThreadPage { items, next })
}

pub fn get_thread(conn: &Connection, thread_id: i64) -> Result<Option<ThreadDetail>> {
    let thread = conn
        .prepare_cached(&format!(
            "SELECT {THREAD_COLS} FROM threads t WHERE t.id = ?1"
        ))?
        .query_row([thread_id], thread_row)
        .optional()?;
    let Some(thread) = thread else {
        return Ok(None);
    };

    let mut messages: Vec<MessageMeta> = conn
        .prepare_cached(
            "SELECT id, from_name, from_email, to_text, subject, snippet, internal_date, \
             is_unread, is_starred, has_attachment, body_state \
             FROM messages WHERE thread_id = ?1 ORDER BY internal_date, id",
        )?
        .query_map([thread_id], |r| {
            Ok(MessageMeta {
                id: r.get(0)?,
                from_name: r.get(1)?,
                from_email: r.get(2)?,
                to_text: r.get(3)?,
                subject: r.get(4)?,
                snippet: r.get(5)?,
                internal_date: r.get(6)?,
                is_unread: r.get(7)?,
                is_starred: r.get(8)?,
                has_attachment: r.get(9)?,
                body_state: BodyState::from_db(r.get(10)?),
                attachments: Vec::new(),
            })
        })?
        .collect::<Result<_>>()?;

    if messages.iter().any(|m| m.has_attachment) {
        let mut st = conn.prepare_cached(
            "SELECT a.id, a.message_id, a.filename, a.mime_type, a.size, a.content_id, \
             a.is_inline, a.local_path \
             FROM messages m JOIN attachments a ON a.message_id = m.id \
             WHERE m.thread_id = ?1 ORDER BY a.id",
        )?;
        let atts = st.query_map([thread_id], |r| {
            Ok(Attachment {
                id: r.get(0)?,
                message_id: r.get(1)?,
                filename: r.get(2)?,
                mime_type: r.get(3)?,
                size: r.get(4)?,
                content_id: r.get(5)?,
                is_inline: r.get(6)?,
                local_path: r.get(7)?,
            })
        })?;
        for a in atts {
            let a = a?;
            if let Some(m) = messages.iter_mut().find(|m| m.id == a.message_id) {
                m.attachments.push(a);
            }
        }
    }

    let label_keys = conn
        .prepare_cached("SELECT label_key FROM thread_labels WHERE thread_id = ?1")?
        .query_map([thread_id], |r| r.get(0))?
        .collect::<Result<_>>()?;

    Ok(Some(ThreadDetail {
        thread,
        messages,
        label_keys,
    }))
}

/// Body of one message, or `None` when it is not stored yet.
pub fn get_body(conn: &Connection, message_id: i64) -> Result<Option<Body>> {
    conn.prepare_cached(
        "SELECT b.message_id, b.html_sanitized, b.text, b.sanitizer_version, \
         COALESCE(sp.load_images, 0) \
         FROM bodies b JOIN messages m ON m.id = b.message_id \
         LEFT JOIN sender_prefs sp ON sp.email = m.from_email \
         WHERE b.message_id = ?1",
    )?
    .query_row([message_id], |r| {
        Ok(Body {
            message_id: r.get(0)?,
            html: r.get(1)?,
            text: r.get(2)?,
            sanitizer_version: r.get(3)?,
            load_images: r.get(4)?,
        })
    })
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(conn: &Connection, sql: &str) -> String {
        let mut st = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let n = st.parameter_count();
        let args: Vec<i64> = vec![1; n];
        let rows = st
            .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(3))
            .unwrap();
        rows.map(|r| r.unwrap()).collect::<Vec<_>>().join("\n")
    }

    fn assert_no_scan(conn: &Connection, sql: &str) {
        let p = plan(conn, sql);
        for line in p.lines() {
            assert!(
                !line.starts_with("SCAN") && !line.contains("TEMP B-TREE"),
                "full scan or sort in plan for {sql}:\n{p}"
            );
        }
    }

    #[test]
    fn list_queries_use_indexes() {
        let conn = crate::open_in_memory().unwrap();
        assert_no_scan(&conn, SQL_LIST_UNIFIED);
        assert_no_scan(&conn, SQL_LIST_ACCOUNT);
    }

    #[test]
    fn thread_and_body_queries_use_indexes() {
        let conn = crate::open_in_memory().unwrap();
        for sql in [
            "SELECT id FROM messages WHERE thread_id = ?1 ORDER BY internal_date, id",
            "SELECT a.id FROM messages m JOIN attachments a ON a.message_id = m.id WHERE m.thread_id = ?1",
            "SELECT label_key FROM thread_labels WHERE thread_id = ?1",
            "SELECT b.text, COALESCE(sp.load_images, 0) FROM bodies b JOIN messages m ON m.id = b.message_id \
             LEFT JOIN sender_prefs sp ON sp.email = m.from_email WHERE b.message_id = ?1",
        ] {
            assert_no_scan(&conn, sql);
        }
    }
}
