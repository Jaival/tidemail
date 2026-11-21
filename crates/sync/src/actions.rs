//! Action queue: optimistic local apply, 5 s undo window, sender loop with
//! exponential backoff, failure handling, reconciliation helper.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tidemail_db::rusqlite::{params, Connection, OptionalExtension};
use tidemail_db::types::{ActionKind, ActionRequest, ActionTicket};

use crate::api::ApiError;
use crate::store::{self, MsgState, Op, Remote};
use crate::{Error, Inner, Result};

/// `action_queue.payload_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub kind: ActionKind,
    pub thread_gmail_id: String,
    pub op: Op,
    /// Label sets before the action; used for undo inside the window.
    pub before: Vec<MsgState>,
}

const RECENT_MAX: usize = 500;
const OFFLINE_RETRY: Duration = Duration::from_secs(10);
const AUTH_RETRY: Duration = Duration::from_secs(60);

fn inverse_kind(k: &ActionKind) -> ActionKind {
    use ActionKind::*;
    match k {
        Archive => Unarchive,
        Unarchive => Archive,
        Trash => Untrash,
        Untrash => Trash,
        Star => Unstar,
        Unstar => Star,
        MarkRead => MarkUnread,
        MarkUnread => MarkRead,
        AddLabel { label_key } => RemoveLabel {
            label_key: label_key.clone(),
        },
        RemoveLabel { label_key } | Move { label_key } => AddLabel {
            label_key: label_key.clone(),
        },
    }
}

fn kind_name(k: &ActionKind) -> String {
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str().map(str::to_owned)))
        .unwrap_or_default()
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_owned()).collect()
}

fn make_op(conn: &Connection, account_id: i64, kind: &ActionKind) -> Result<Op> {
    use ActionKind::*;
    let modify = |add: Vec<String>, remove: Vec<String>| Op {
        add,
        remove,
        remote: Remote::Modify,
    };
    Ok(match kind {
        Archive => modify(vec![], s(&["INBOX"])),
        Unarchive => modify(s(&["INBOX"]), vec![]),
        Trash => Op {
            add: s(&["TRASH"]),
            remove: s(&["INBOX"]),
            remote: Remote::Trash,
        },
        Untrash => Op {
            add: vec![],
            remove: s(&["TRASH"]),
            remote: Remote::Untrash,
        },
        Star => modify(s(&["STARRED"]), vec![]),
        Unstar => modify(vec![], s(&["STARRED"])),
        MarkRead => modify(vec![], s(&["UNREAD"])),
        MarkUnread => modify(s(&["UNREAD"]), vec![]),
        AddLabel { label_key } => modify(
            vec![store::gmail_label_id(conn, account_id, label_key)?],
            vec![],
        ),
        RemoveLabel { label_key } => modify(
            vec![],
            vec![store::gmail_label_id(conn, account_id, label_key)?],
        ),
        Move { label_key } => modify(
            vec![store::gmail_label_id(conn, account_id, label_key)?],
            s(&["INBOX"]),
        ),
    })
}

fn insert_row(
    conn: &Connection,
    account_id: i64,
    thread_id: i64,
    p: &Payload,
    next_attempt_at: i64,
    now: i64,
) -> Result<i64> {
    conn.prepare_cached(
        "INSERT INTO action_queue (account_id, thread_id, kind, payload_json, attempts, \
         next_attempt_at, created_at) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
    )?
    .execute(params![
        account_id,
        thread_id,
        kind_name(&p.kind),
        serde_json::to_string(p)?,
        next_attempt_at,
        now
    ])?;
    Ok(conn.last_insert_rowid())
}

/// Reconciliation: after a remote change for a thread, apply its pending
/// (not failed) actions on top, oldest first.
pub fn reapply_pending(conn: &Connection, thread_id: i64) -> Result<()> {
    let rows: Vec<(i64, String)> = conn
        .prepare_cached(
            "SELECT account_id, payload_json FROM action_queue \
             WHERE thread_id = ?1 AND failed = 0 ORDER BY id",
        )?
        .query_map([thread_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    for (account_id, json) in rows {
        let p: Payload = serde_json::from_str(&json)?;
        store::apply_op(conn, account_id, thread_id, &p.op)?;
    }
    Ok(())
}

fn backoff(attempts: u32) -> Duration {
    Duration::from_secs((5u64 << attempts.saturating_sub(1).min(6)).min(300))
}

impl Inner {
    pub fn apply_action(&self, req: ActionRequest) -> Result<ActionTicket> {
        let now = self.clock.now_ms();
        let due = now + self.cfg.undo_delay.as_millis() as i64;
        let mut keys = BTreeSet::new();
        let mut ids = Vec::new();
        {
            let mut conn = self.fg();
            let tx = conn.transaction()?;
            for &tid in &req.thread_ids {
                let (account_id, gmail_id): (i64, String) = tx
                    .prepare_cached("SELECT account_id, gmail_id FROM threads WHERE id = ?1")?
                    .query_row([tid], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?
                    .ok_or_else(|| Error::NotFound(format!("thread {tid}")))?;
                keys.extend(store::thread_label_keys(&tx, tid)?);
                let p = Payload {
                    op: make_op(&tx, account_id, &req.kind)?,
                    kind: req.kind.clone(),
                    thread_gmail_id: gmail_id,
                    before: store::snapshot(&tx, tid)?,
                };
                store::apply_op(&tx, account_id, tid, &p.op)?;
                keys.extend(store::thread_label_keys(&tx, tid)?);
                let id = insert_row(&tx, account_id, tid, &p, due, now)?;
                self.remember(id, account_id, tid, p);
                ids.push(id);
            }
            tx.commit()?;
        }
        self.notifier
            .data_changed(req.thread_ids, keys.into_iter().collect());
        self.notifier.status_changed(self.status());
        self.sender_wake.notify_one();
        Ok(ActionTicket { action_ids: ids })
    }

    fn remember(&self, id: i64, account_id: i64, thread_id: i64, p: Payload) {
        let mut recent = self.recent.lock().expect("lock");
        recent.insert(id, (account_id, thread_id, p));
        while recent.len() > RECENT_MAX {
            recent.pop_first();
        }
    }

    pub fn undo_action(&self, action_ids: &[i64]) -> Result<()> {
        let now = self.clock.now_ms();
        let mut threads = Vec::new();
        let mut keys = BTreeSet::new();
        {
            let mut conn = self.fg();
            let tx = conn.transaction()?;
            for &id in action_ids {
                let row: Option<(i64, i64, String, i64, i64)> = tx
                    .prepare_cached(
                        "SELECT account_id, thread_id, payload_json, attempts, next_attempt_at \
                         FROM action_queue WHERE id = ?1",
                    )?
                    .query_row([id], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                    })
                    .optional()?;
                let recent = self.recent.lock().expect("lock").get(&id).cloned();
                let (account_id, thread_id, p, in_window) = match (row, recent) {
                    (Some((a, t, json, attempts, due)), _) => (
                        a,
                        t,
                        serde_json::from_str::<Payload>(&json)?,
                        attempts == 0 && due > now,
                    ),
                    (None, Some((a, th, p))) => (a, th, p, false),
                    (None, None) => return Err(Error::NotFound(format!("action {id}"))),
                };
                keys.extend(store::thread_label_keys(&tx, thread_id)?);
                if in_window {
                    tx.execute("DELETE FROM action_queue WHERE id = ?1", [id])?;
                    store::restore_snapshot(&tx, account_id, thread_id, &p.before)?;
                } else {
                    let inv = Payload {
                        kind: inverse_kind(&p.kind),
                        thread_gmail_id: p.thread_gmail_id.clone(),
                        op: p.op.inverse(),
                        before: vec![],
                    };
                    store::apply_op(&tx, account_id, thread_id, &inv.op)?;
                    let new_id = insert_row(&tx, account_id, thread_id, &inv, now, now)?;
                    self.remember(new_id, account_id, thread_id, inv);
                }
                keys.extend(store::thread_label_keys(&tx, thread_id)?);
                threads.push(thread_id);
                self.recent.lock().expect("lock").remove(&id);
            }
            tx.commit()?;
        }
        self.notifier
            .data_changed(threads, keys.into_iter().collect());
        self.notifier.status_changed(self.status());
        self.sender_wake.notify_one();
        Ok(())
    }

    pub fn retry_failed(&self, action_id: i64) -> Result<()> {
        let now = self.clock.now_ms();
        let thread_id = {
            let mut conn = self.fg();
            let tx = conn.transaction()?;
            let (account_id, thread_id, json): (i64, i64, String) = tx
                .prepare_cached(
                    "SELECT account_id, thread_id, payload_json FROM action_queue \
                     WHERE id = ?1 AND failed = 1",
                )?
                .query_row([action_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()?
                .ok_or_else(|| Error::NotFound(format!("failed action {action_id}")))?;
            tx.execute(
                "UPDATE action_queue SET failed = 0, attempts = 0, next_attempt_at = ?2, \
                 last_error = NULL WHERE id = ?1",
                params![action_id, now],
            )?;
            // The failure restored Gmail state locally; show the action again.
            let p: Payload = serde_json::from_str(&json)?;
            store::apply_op(&tx, account_id, thread_id, &p.op)?;
            tx.commit()?;
            thread_id
        };
        let keys = store::thread_label_keys(&self.fg(), thread_id)?;
        self.notifier.data_changed(vec![thread_id], keys);
        self.notifier.status_changed(self.status());
        self.sender_wake.notify_one();
        Ok(())
    }

    pub fn dismiss_failed(&self, action_id: i64) -> Result<()> {
        let row: Option<(i64, i64)> = {
            let conn = self.fg();
            let row = conn
                .prepare_cached(
                    "SELECT account_id, thread_id FROM action_queue WHERE id = ?1 AND failed = 1",
                )?
                .query_row([action_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            conn.execute(
                "DELETE FROM action_queue WHERE id = ?1 AND failed = 1",
                [action_id],
            )?;
            row
        };
        if let Some((account_id, thread_id)) = row {
            self.request_restore(account_id, thread_id);
        }
        self.notifier.status_changed(self.status());
        Ok(())
    }

    /// Next unblocked row: (id, next_attempt_at). A row waits for earlier
    /// non-failed rows of the same thread.
    fn next_row(&self, db: &Mutex<Connection>) -> Result<Option<(i64, i64)>> {
        let conn = db.lock().expect("lock");
        let row = conn
            .prepare_cached(
                "SELECT q.id, q.next_attempt_at FROM action_queue q WHERE q.failed = 0 \
                 AND NOT EXISTS (SELECT 1 FROM action_queue e WHERE e.thread_id = q.thread_id \
                 AND e.failed = 0 AND e.id < q.id) ORDER BY q.next_attempt_at, q.id LIMIT 1",
            )?
            .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        Ok(row)
    }

    async fn send_one(&self, db: &Mutex<Connection>, id: i64) -> Result<()> {
        let (account_id, thread_id, json, attempts, api) = {
            let conn = db.lock().expect("lock");
            let (account_id, thread_id, json, attempts): (i64, i64, String, u32) = conn
            .prepare_cached(
                "SELECT account_id, thread_id, payload_json, attempts FROM action_queue WHERE id = ?1",
            )?
            .query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            conn.execute(
                "UPDATE action_queue SET attempts = ?2 WHERE id = ?1",
                params![id, attempts + 1],
            )?;
            let api = self.api_for(&conn, account_id);
            (account_id, thread_id, json, attempts, api)
        };
        let attempt = attempts + 1;
        let p: Payload = serde_json::from_str(&json)?;

        let result = match api {
            Err(e) => Err(e),
            Ok(api) => match p.op.remote {
                Remote::Modify => {
                    api.threads_modify(&p.thread_gmail_id, &p.op.add, &p.op.remove)
                        .await
                }
                Remote::Trash => api.threads_trash(&p.thread_gmail_id).await,
                Remote::Untrash => api.threads_untrash(&p.thread_gmail_id).await,
            },
        };
        let now = self.clock.now_ms();
        let conn = db.lock().expect("lock");
        match result {
            Ok(()) => {
                conn.execute("DELETE FROM action_queue WHERE id = ?1", [id])?;
            }
            Err(e @ (ApiError::Offline(_) | ApiError::Auth(_))) => {
                // Not the action's fault: do not use up an attempt.
                let wait = if matches!(e, ApiError::Auth(_)) {
                    AUTH_RETRY
                } else {
                    OFFLINE_RETRY
                };
                conn.execute(
                    "UPDATE action_queue SET attempts = ?2, next_attempt_at = ?3, last_error = ?4 \
                     WHERE id = ?1",
                    params![id, attempts, now + wait.as_millis() as i64, e.to_string()],
                )?;
            }
            Err(e) if attempt >= self.cfg.max_attempts => {
                conn.execute(
                    "UPDATE action_queue SET failed = 1, last_error = ?2 WHERE id = ?1",
                    params![id, e.to_string()],
                )?;
                // Next sync restores Gmail's state for this thread.
                self.request_restore(account_id, thread_id);
            }
            Err(e) => {
                conn.execute(
                    "UPDATE action_queue SET next_attempt_at = ?2, last_error = ?3 WHERE id = ?1",
                    params![id, now + backoff(attempt).as_millis() as i64, e.to_string()],
                )?;
            }
        }
        drop(conn);
        self.notifier.status_changed(self.status());
        Ok(())
    }
}

/// Background sender: one task for all accounts.
pub async fn sender_loop(inner: std::sync::Arc<Inner>) {
    let conn = match tidemail_db::open(&inner.db_path) {
        Ok(c) => Mutex::new(c),
        Err(e) => {
            eprintln!("tidemail-sync: sender cannot open db: {e}");
            return;
        }
    };
    loop {
        match inner.next_row(&conn) {
            Ok(Some((id, due))) => {
                let now = inner.clock.now_ms();
                if due <= now {
                    if let Err(e) = inner.send_one(&conn, id).await {
                        eprintln!("tidemail-sync: sender: {e}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis((due - now) as u64)) => {}
                        _ = inner.sender_wake.notified() => {}
                    }
                }
            }
            Ok(None) => inner.sender_wake.notified().await,
            Err(e) => {
                eprintln!("tidemail-sync: sender: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
