//! `messages_fts` is external content over the view `fts_src`. Views have no
//! triggers, so every writer calls these functions:
//! - message metadata arrives: `insert` with an empty body,
//! - body arrives: `replace_body` (delete old row, insert new),
//! - message deleted: `delete` (before deleting the `messages` row).
//!
//! The values passed to `delete` must equal the values that were indexed.

use tidemail_db::rusqlite::{params, Connection};
use tidemail_db::Result;

pub struct FtsRow<'a> {
    pub from_name: &'a str,
    pub from_email: &'a str,
    pub to_text: &'a str,
    pub subject: &'a str,
    pub body_text: &'a str,
}

pub fn insert(conn: &Connection, message_id: i64, r: &FtsRow) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO messages_fts (rowid, from_name, from_email, to_text, subject, body_text) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![
        message_id,
        r.from_name,
        r.from_email,
        r.to_text,
        r.subject,
        r.body_text
    ])?;
    Ok(())
}

pub fn delete(conn: &Connection, message_id: i64, r: &FtsRow) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO messages_fts (messages_fts, rowid, from_name, from_email, to_text, subject, body_text) \
         VALUES ('delete', ?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![message_id, r.from_name, r.from_email, r.to_text, r.subject, r.body_text])?;
    Ok(())
}

/// Re-index a message whose body changed. `old` holds the indexed values.
pub fn replace_body(
    conn: &Connection,
    message_id: i64,
    old: &FtsRow,
    new_body: &str,
) -> Result<()> {
    delete(conn, message_id, old)?;
    insert(
        conn,
        message_id,
        &FtsRow {
            body_text: new_body,
            ..*old
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidemail_db::types::BodyState;
    use tidemail_db::write::*;

    #[test]
    fn insert_then_replace_body() {
        let conn = tidemail_db::open_in_memory().unwrap();
        let a = insert_account(
            &conn,
            &NewAccount {
                email: "a@x",
                display_name: None,
                color: "#000",
                sort_order: 0,
            },
        )
        .unwrap();
        let t = ensure_thread(&conn, a, "t").unwrap();
        let m = insert_message(
            &conn,
            &NewMessage {
                account_id: a,
                gmail_id: "m",
                thread_id: t,
                from_name: "Zoë",
                from_email: "zoe@x",
                to_text: "",
                subject: "Quarterly report",
                snippet: "",
                internal_date: 1,
                is_unread: false,
                is_starred: false,
                has_attachment: false,
                body_state: BodyState::Missing,
            },
        )
        .unwrap();
        let row = FtsRow {
            from_name: "Zoë",
            from_email: "zoe@x",
            to_text: "",
            subject: "Quarterly report",
            body_text: "",
        };
        insert(&conn, m, &row).unwrap();
        upsert_body(&conn, m, None, "pineapple budget", 1).unwrap();
        replace_body(&conn, m, &row, "pineapple budget").unwrap();

        let hits = |q: &str| -> i64 {
            conn.query_row(
                "SELECT count(*) FROM messages_fts WHERE messages_fts MATCH ?1",
                [q],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(hits("pineapple"), 1);
        assert_eq!(hits("zoe"), 1); // remove_diacritics
        assert_eq!(hits("qu*"), 1); // prefix index
        let snip: String = conn
            .query_row(
                "SELECT snippet(messages_fts, 4, '<mark>', '</mark>', '…', 8) FROM messages_fts \
                 WHERE messages_fts MATCH 'budget'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(snip.contains("<mark>budget</mark>"));
        conn.execute(
            "INSERT INTO messages_fts(messages_fts) VALUES('integrity-check')",
            [],
        )
        .unwrap();
    }
}
