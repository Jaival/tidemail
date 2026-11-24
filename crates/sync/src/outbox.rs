//! Send queue (ADR 0004): the sent copy goes into `messages` at once with
//! `SENT` (gmail id `local-<outbox id>`), `messages.send` runs after the
//! 10 s undo window, offline mail waits and retries, and Gmail's ids replace
//! the local ones when it confirms. Rows survive a restart: mail queued
//! before quit goes out on the next launch.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tidemail_db::rusqlite::{params, Connection, OptionalExtension};
use tidemail_db::types::{BodyState, FailedSend, OutgoingMail, SendTicket};
use tidemail_db::write::{self, NewMessage};
use tidemail_gmail::compose::{parse_addresses, Mailbox, PlainMessage};
use tidemail_search::fts::{self, FtsRow};

use crate::api::{ApiError, MessageRef};
use crate::{store, Error, Inner, Result};

const OFFLINE_RETRY: Duration = Duration::from_secs(10);
const AUTH_RETRY: Duration = Duration::from_secs(60);
/// A send that keeps failing is shown quickly (a bad request will not heal).
const MAX_ATTEMPTS: u32 = 3;

/// Gmail id of the sent copy before Gmail confirms; full diff skips these.
pub const LOCAL_PREFIX: &str = "local-";

fn backoff(attempts: u32) -> Duration {
    Duration::from_secs((5u64 << attempts.saturating_sub(1).min(6)).min(300))
}

fn snippet(body: &str) -> String {
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(200).collect()
}

/// Remove the local copy of a queued message and its outbox row.
fn delete_local(conn: &Connection, id: i64) -> Result<Option<(OutgoingMail, i64)>> {
    let row: Option<(i64, String)> = conn
        .prepare_cached("SELECT account_id, mail_json FROM outbox WHERE id = ?1")?
        .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((account_id, json)) = row else {
        return Ok(None);
    };
    conn.execute("DELETE FROM outbox WHERE id = ?1", [id])?;
    let local = format!("{LOCAL_PREFIX}{id}");
    let thread = store::delete_message(conn, account_id, &local)?.unwrap_or(-1);
    if thread >= 0 {
        write::recompute_thread(conn, thread)?;
    }
    Ok(Some((serde_json::from_str(&json)?, thread)))
}

/// Gmail confirmed: its ids replace the local ones. If the sync already
/// stored Gmail's copy, the local one goes.
fn confirm(conn: &Connection, id: i64, account_id: i64, sent: &MessageRef) -> Result<Vec<i64>> {
    let local = format!("{LOCAL_PREFIX}{id}");
    conn.execute("DELETE FROM outbox WHERE id = ?1", [id])?;
    let row: Option<(i64, i64)> = conn
        .prepare_cached("SELECT id, thread_id FROM messages WHERE account_id = ?1 AND gmail_id = ?2")?
        .query_row(params![account_id, local], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((message, local_thread)) = row else {
        return Ok(vec![]);
    };
    let synced = conn
        .prepare_cached("SELECT 1 FROM messages WHERE account_id = ?1 AND gmail_id = ?2")?
        .query_row(params![account_id, sent.id], |_| Ok(()))
        .optional()?
        .is_some();
    if synced {
        store::delete_message(conn, account_id, &local)?;
        write::recompute_thread(conn, local_thread)?;
        return Ok(vec![local_thread]);
    }
    conn.execute(
        "UPDATE messages SET gmail_id = ?2 WHERE id = ?1",
        params![message, sent.id],
    )?;
    let existing: Option<i64> = conn
        .prepare_cached("SELECT id FROM threads WHERE account_id = ?1 AND gmail_id = ?2")?
        .query_row(params![account_id, sent.thread_id], |r| r.get(0))
        .optional()?;
    match existing {
        Some(t) if t != local_thread => {
            conn.execute(
                "UPDATE messages SET thread_id = ?2 WHERE id = ?1",
                params![message, t],
            )?;
            write::recompute_thread(conn, t)?;
            write::recompute_thread(conn, local_thread)?;
            Ok(vec![local_thread, t])
        }
        _ => {
            conn.execute(
                "UPDATE threads SET gmail_id = ?2 WHERE id = ?1",
                params![local_thread, sent.thread_id],
            )?;
            Ok(vec![local_thread])
        }
    }
}

/// `pending, queued_offline, failed` for `SyncStatus`.
pub fn status(conn: &Connection) -> Result<(u32, u32, Vec<FailedSend>)> {
    let (pending, offline): (u32, u32) = conn
        .prepare_cached(
            "SELECT count(*), COALESCE(sum(offline), 0) FROM outbox WHERE failed = 0",
        )?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let failed = conn
        .prepare_cached(
            "SELECT id, mail_json, COALESCE(last_error, '') FROM outbox WHERE failed = 1 ORDER BY id",
        )?
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get(2)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(id, json, error)| FailedSend {
            id,
            subject: serde_json::from_str::<OutgoingMail>(&json)
                .map(|m| m.subject)
                .unwrap_or_default(),
            error,
        })
        .collect();
    Ok((pending, offline, failed))
}

impl Inner {
    /// Build the message, store the sent copy and queue it. Address errors
    /// come back as `Error::Invalid` before anything is stored.
    pub fn queue_send(&self, mail: OutgoingMail) -> Result<SendTicket> {
        let list = |s: &str| parse_addresses(s).map_err(Error::Invalid);
        let (to, cc, bcc) = (list(&mail.to)?, list(&mail.cc)?, list(&mail.bcc)?);
        if to.is_empty() && cc.is_empty() && bcc.is_empty() {
            return Err(Error::Invalid("Add at least one recipient".into()));
        }
        let now = self.clock.now_ms();
        let due = now + self.cfg.send_delay.as_millis() as i64;
        let mut conn = self.fg();
        let tx = conn.transaction()?;
        let (email, name): (String, Option<String>) = tx
            .prepare_cached("SELECT email, display_name FROM accounts WHERE id = ?1")?
            .query_row([mail.account_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("account {}", mail.account_id)))?;
        let from = Mailbox {
            name: name.unwrap_or_default(),
            email,
        };
        let raw = PlainMessage {
            from: &from,
            to: &to,
            cc: &cc,
            bcc: &bcc,
            subject: &mail.subject,
            body: &mail.body,
            date_ms: now,
        }
        .to_rfc822();
        tx.prepare_cached(
            "INSERT INTO outbox (account_id, mail_json, raw, next_attempt_at, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?
        .execute(params![mail.account_id, serde_json::to_string(&mail)?, raw, due, now])?;
        let id = tx.last_insert_rowid();
        let local = format!("{LOCAL_PREFIX}{id}");
        let thread_id = write::ensure_thread(&tx, mail.account_id, &local)?;
        let to_text = to
            .iter()
            .chain(&cc)
            .map(Mailbox::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let snippet = snippet(&mail.body);
        let message_id = write::insert_message(
            &tx,
            &NewMessage {
                account_id: mail.account_id,
                gmail_id: &local,
                thread_id,
                from_name: &from.name,
                from_email: &from.email,
                to_text: &to_text,
                subject: &mail.subject,
                snippet: &snippet,
                internal_date: now,
                is_unread: false,
                is_starred: false,
                has_attachment: false,
                body_state: BodyState::Stored,
            },
        )?;
        write::upsert_body(&tx, message_id, None, &mail.body, tidemail_render::SANITIZER_VERSION)?;
        fts::insert(
            &tx,
            message_id,
            &FtsRow {
                from_name: &from.name,
                from_email: &from.email.to_ascii_lowercase(),
                to_text: &to_text,
                subject: &mail.subject,
                body_text: &mail.body,
            },
        )?;
        store::apply_op(
            &tx,
            mail.account_id,
            thread_id,
            &store::Op {
                add: vec!["SENT".into()],
                remove: vec![],
                remote: store::Remote::Modify,
            },
        )?;
        tx.execute(
            "UPDATE outbox SET message_id = ?2 WHERE id = ?1",
            params![id, message_id],
        )?;
        tx.commit()?;
        drop(conn);
        self.notifier.data_changed(vec![thread_id], vec!["SENT".into()]);
        self.notifier.status_changed(self.status());
        self.outbox_wake.notify_one();
        Ok(SendTicket {
            outbox_id: id,
            thread_id,
        })
    }

    /// Take a message back out of the queue (undo, or edit after a
    /// failure): deletes the sent copy and returns what compose had. `None`
    /// when it is being sent or already sent.
    pub fn cancel_send(&self, id: i64) -> Result<Option<OutgoingMail>> {
        let mut conn = self.fg();
        let tx = conn.transaction()?;
        // attempts > 0 and not failed: in flight or waiting for a retry
        // after an error that may have reached Gmail.
        let cancellable = tx
            .prepare_cached("SELECT 1 FROM outbox WHERE id = ?1 AND (attempts = 0 OR failed = 1)")?
            .query_row([id], |_| Ok(()))
            .optional()?
            .is_some();
        if !cancellable {
            return Ok(None);
        }
        let taken = delete_local(&tx, id)?;
        tx.commit()?;
        drop(conn);
        let Some((mail, thread)) = taken else {
            return Ok(None);
        };
        self.notifier.data_changed(vec![thread], vec!["SENT".into()]);
        self.notifier.status_changed(self.status());
        Ok(Some(mail))
    }

    pub fn retry_send(&self, id: i64) -> Result<()> {
        let now = self.clock.now_ms();
        let thread: Option<i64> = {
            let conn = self.fg();
            conn.execute(
                "UPDATE outbox SET failed = 0, attempts = 0, offline = 0, next_attempt_at = ?2, \
                 last_error = NULL WHERE id = ?1 AND failed = 1",
                params![id, now],
            )?;
            let thread = conn
                .prepare_cached(
                    "SELECT m.thread_id FROM outbox o JOIN messages m ON m.id = o.message_id                      WHERE o.id = ?1",
                )?
                .query_row([id], |r| r.get(0))
                .optional()?;
            thread
        };
        self.notifier.data_changed(thread.into_iter().collect(), vec![]);
        self.notifier.status_changed(self.status());
        self.outbox_wake.notify_one();
        Ok(())
    }

    fn next_send(&self, db: &Mutex<Connection>) -> Result<Option<(i64, i64)>> {
        let conn = db.lock().expect("lock");
        let row = conn
            .prepare_cached(
                "SELECT id, next_attempt_at FROM outbox WHERE failed = 0 \
                 ORDER BY next_attempt_at, id LIMIT 1",
            )?
            .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        Ok(row)
    }

    async fn send_queued(&self, db: &Mutex<Connection>, id: i64) -> Result<()> {
        let (account_id, raw, attempts, api) = {
            let conn = db.lock().expect("lock");
            let (account_id, raw, attempts): (i64, Vec<u8>, u32) = conn
                .prepare_cached("SELECT account_id, raw, attempts FROM outbox WHERE id = ?1")?
                .query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            // Marks it in flight: `cancel_send` refuses from here on.
            conn.execute(
                "UPDATE outbox SET attempts = ?2 WHERE id = ?1",
                params![id, attempts + 1],
            )?;
            let api = self.api_for(&conn, account_id);
            (account_id, raw, attempts, api)
        };
        let result = match api {
            Ok(api) => api.messages_send(&raw, None).await,
            Err(e) => Err(e),
        };
        let now = self.clock.now_ms();
        let threads = {
            let mut conn = db.lock().expect("lock");
            let thread_of = |c: &Connection| -> Result<Vec<i64>> {
                Ok(c.prepare_cached(
                    "SELECT m.thread_id FROM outbox o JOIN messages m ON m.id = o.message_id \
                     WHERE o.id = ?1",
                )?
                .query_row([id], |r| r.get(0))
                .optional()?
                .into_iter()
                .collect())
            };
            match result {
                Ok(sent) => {
                    let tx = conn.transaction()?;
                    let t = confirm(&tx, id, account_id, &sent)?;
                    tx.commit()?;
                    t
                }
                Err(e @ (ApiError::Offline(_) | ApiError::Auth(_))) => {
                    // Not the message's fault: the attempt does not count.
                    let wait = match e {
                        ApiError::Auth(_) => AUTH_RETRY,
                        _ => OFFLINE_RETRY,
                    };
                    conn.execute(
                        "UPDATE outbox SET attempts = ?2, offline = 1, next_attempt_at = ?3, \
                         last_error = ?4 WHERE id = ?1",
                        params![id, attempts, now + wait.as_millis() as i64, e.to_string()],
                    )?;
                    thread_of(&conn)?
                }
                Err(e) if attempts + 1 >= MAX_ATTEMPTS => {
                    conn.execute(
                        "UPDATE outbox SET failed = 1, offline = 0, last_error = ?2 WHERE id = ?1",
                        params![id, e.to_string()],
                    )?;
                    thread_of(&conn)?
                }
                Err(e) => {
                    conn.execute(
                        "UPDATE outbox SET offline = 0, next_attempt_at = ?2, last_error = ?3 \
                         WHERE id = ?1",
                        params![id, now + backoff(attempts + 1).as_millis() as i64, e.to_string()],
                    )?;
                    thread_of(&conn)?
                }
            }
        };
        self.notifier.data_changed(threads, vec!["SENT".into()]);
        self.notifier.status_changed(self.status());
        Ok(())
    }
}

/// Background sender for the outbox: one task for all accounts.
pub async fn outbox_loop(inner: Arc<Inner>) {
    let conn = match tidemail_db::open(&inner.db_path) {
        Ok(c) => Mutex::new(c),
        Err(e) => {
            eprintln!("tidemail-sync: outbox cannot open db: {e}");
            return;
        }
    };
    loop {
        match inner.next_send(&conn) {
            Ok(Some((id, due))) => {
                let now = inner.clock.now_ms();
                if due <= now {
                    if let Err(e) = inner.send_queued(&conn, id).await {
                        eprintln!("tidemail-sync: outbox: {e}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis((due - now) as u64)) => {}
                        _ = inner.outbox_wake.notified() => {}
                    }
                }
            }
            Ok(None) => inner.outbox_wake.notified().await,
            Err(e) => {
                eprintln!("tidemail-sync: outbox: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
