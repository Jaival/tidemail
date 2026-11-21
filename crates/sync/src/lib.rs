//! Sync engine: backfill, `history.list` polling, full id diff, action queue
//! (undo delay, reconciliation, failure), body prefetch.
//!
//! Gmail is reached only through the traits in [`api`]; the app crate adapts
//! `tidemail_gmail` to them. All SQL for the engine lives in [`store`] and
//! the modules here (move to `tidemail_db` on merge where shared).
//!
//! Threading: [`Engine`] methods are sync and use a foreground SQLite
//! connection. Background tasks (sender, one sync loop per account, prefetch)
//! run on tokio and each own a connection to the same WAL file.

mod actions;
pub mod api;
#[cfg(test)]
mod fake;
mod store;
mod syncer;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tidemail_db::rusqlite::{params, Connection, OptionalExtension};
use tidemail_db::types::{
    Account, AccountSync, AccountSyncState, ActionRequest, ActionTicket, Backfill, FailedAction,
    SyncStatus,
};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, Notify};

use crate::api::{ApiError, GmailApi, GmailProvider};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Db(#[from] tidemail_db::Error),
    #[error(transparent)]
    Gmail(#[from] tidemail_gmail::Error),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Push channel to the UI. The app crate implements it with Tauri events.
pub trait Notifier: Send + Sync + 'static {
    /// Rows changed; the UI re-queries what is on screen.
    fn data_changed(&self, thread_ids: Vec<i64>, label_keys: Vec<String>);
    fn status_changed(&self, status: SyncStatus);
}

/// Tunables. `Default` holds the spec values.
#[derive(Debug, Clone)]
pub struct Config {
    /// `history.list` poll period per account.
    pub poll_interval: Duration,
    /// Delay before an action's first send (the undo window).
    pub undo_delay: Duration,
    pub max_attempts: u32,
    /// Pause after each `messages.get` batch (quota: 250 units/s, a 50-batch
    /// of metadata gets costs 250).
    pub batch_gap: Duration,
    /// Above this many messages, bodies older than `body_window` are deferred
    /// (fetched when opened). Default: every account, 6 months (decided
    /// 2026-11-21: all metadata, recent bodies).
    pub body_policy_threshold: u64,
    pub body_window: Duration,
    /// A new account's backfill lists only this much recent mail; settings
    /// extend it (`Engine::extend_sync`). `None`: all mail. Default 183 days
    /// (decided 2026-11-21).
    pub sync_window: Option<Duration>,
    /// Message ids queued for prefetch before new ones are dropped.
    pub prefetch_queue: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(7),
            undo_delay: Duration::from_secs(5),
            max_attempts: 8,
            batch_gap: Duration::from_secs(1),
            body_policy_threshold: 0,
            body_window: Duration::from_secs(183 * 24 * 3600),
            sync_window: Some(Duration::from_secs(183 * 24 * 3600)),
            prefetch_queue: 512,
        }
    }
}

/// Wall time that follows tokio's clock, so paused-time tests can drive it.
pub(crate) struct Clock {
    base_ms: i64,
    start: tokio::time::Instant,
}

impl Clock {
    fn new() -> Self {
        Self {
            base_ms: tidemail_db::now_ms(),
            start: tokio::time::Instant::now(),
        }
    }
    pub fn now_ms(&self) -> i64 {
        self.base_ms + self.start.elapsed().as_millis() as i64
    }
}

pub(crate) struct Inner {
    pub db_path: PathBuf,
    pub notifier: Arc<dyn Notifier>,
    pub provider: Option<Arc<dyn GmailProvider>>,
    pub cfg: Config,
    pub clock: Clock,
    /// Foreground connection (Engine methods).
    pub fg: Mutex<Connection>,
    pub states: Mutex<HashMap<i64, AccountSyncState>>,
    pub sender_wake: Notify,
    pub account_wake: Mutex<HashMap<i64, Arc<Notify>>>,
    /// Threads whose Gmail state must be re-fetched (failed / dismissed actions).
    pub restore: Mutex<HashMap<i64, HashSet<i64>>>,
    /// Recently created actions, for undo after the row is gone.
    pub recent: Mutex<BTreeMap<i64, (i64, i64, actions::Payload)>>,
    pub prefetch_tx: mpsc::Sender<i64>,
    pub tasks: Mutex<HashMap<i64, tokio::task::JoinHandle<()>>>,
    pub handle: Handle,
}

pub struct Engine {
    inner: Arc<Inner>,
    runtime: Option<Runtime>,
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_background();
        }
    }
}

impl Engine {
    /// Open own DB connections and start background tasks. Must return at
    /// once: it is on the cold-launch path. Without a Gmail provider the
    /// engine only applies actions locally (no network tasks run); use
    /// [`Engine::start_with`] to inject the provider.
    pub fn start(db_path: PathBuf, notifier: Arc<dyn Notifier>) -> Self {
        Self::start_with(db_path, notifier, None, Config::default())
    }

    /// Like [`Engine::start`] with the sign-in / API seam and tunables.
    /// Runs its tasks on its own small tokio runtime.
    pub fn start_with(
        db_path: PathBuf,
        notifier: Arc<dyn Notifier>,
        provider: Option<Arc<dyn GmailProvider>>,
        cfg: Config,
    ) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("tidemail-sync")
            .enable_all()
            .build()
            .expect("tokio runtime");
        let mut engine = Self::start_on(runtime.handle().clone(), db_path, notifier, provider, cfg);
        engine.runtime = Some(runtime);
        engine
    }

    /// Run on an existing tokio runtime (tests).
    pub fn start_on(
        handle: Handle,
        db_path: PathBuf,
        notifier: Arc<dyn Notifier>,
        provider: Option<Arc<dyn GmailProvider>>,
        cfg: Config,
    ) -> Self {
        let fg = tidemail_db::open(&db_path).expect("open database");
        let (prefetch_tx, prefetch_rx) = mpsc::channel(cfg.prefetch_queue.max(1));
        let inner = Arc::new(Inner {
            db_path,
            notifier,
            provider,
            cfg,
            clock: Clock::new(),
            fg: Mutex::new(fg),
            states: Mutex::default(),
            sender_wake: Notify::new(),
            account_wake: Mutex::default(),
            restore: Mutex::default(),
            recent: Mutex::default(),
            prefetch_tx,
            tasks: Mutex::default(),
            handle: handle.clone(),
        });
        if inner.provider.is_some() {
            let i = inner.clone();
            handle.spawn(async move {
                let ids: Vec<i64> = {
                    let conn = i.fg();
                    let Ok(mut st) = conn.prepare("SELECT id FROM accounts ORDER BY sort_order")
                    else {
                        return;
                    };
                    let Ok(rows) = st.query_map([], |r| r.get(0)) else {
                        return;
                    };
                    rows.flatten().collect()
                };
                for id in ids {
                    i.spawn_account(id);
                }
                tokio::spawn(actions::sender_loop(i.clone()));
                tokio::spawn(syncer::prefetch_loop(i, prefetch_rx));
            });
        }
        Self {
            inner,
            runtime: None,
        }
    }

    /// Apply locally at once, enqueue for Gmail with a 5 s undo delay.
    pub fn apply_action(&self, req: ActionRequest) -> Result<ActionTicket> {
        self.inner.apply_action(req)
    }

    /// Inside 5 s: delete the queue rows. Later: enqueue the inverse.
    pub fn undo_action(&self, action_ids: &[i64]) -> Result<()> {
        self.inner.undo_action(action_ids)
    }

    pub fn retry_failed(&self, action_id: i64) -> Result<()> {
        self.inner.retry_failed(action_id)
    }

    pub fn dismiss_failed(&self, action_id: i64) -> Result<()> {
        self.inner.dismiss_failed(action_id)
    }

    /// Fetch bodies for these messages soon (next 10 in the list). Ids beyond
    /// the bounded queue are dropped.
    pub fn prefetch_bodies(&self, message_ids: &[i64]) {
        for id in message_ids {
            if self.inner.prefetch_tx.try_send(*id).is_err() {
                break;
            }
        }
    }

    pub fn status(&self) -> SyncStatus {
        self.inner.status()
    }

    /// Download older mail: the backfill lists back to `from` (epoch ms;
    /// `None`: all mail). Only widens the window. The backfill starts again
    /// from the newest page and skips ids already stored.
    pub fn extend_sync(&self, account_id: i64, from: Option<i64>) -> Result<()> {
        let changed = self.inner.fg().execute(
            "UPDATE sync_state SET sync_from = ?2, backfill_cursor = NULL \
             WHERE account_id = ?1 AND sync_from IS NOT NULL AND (?2 IS NULL OR ?2 < sync_from)",
            params![account_id, from],
        )?;
        if changed > 0 {
            self.inner.notifier.status_changed(self.inner.status());
            self.inner.account_wake(account_id).notify_one();
        }
        Ok(())
    }

    /// Browser sign-in, insert the account, start its backfill.
    pub async fn add_account(&self) -> Result<Account> {
        let inner = self.inner.clone();
        inner
            .handle
            .clone()
            .spawn(async move { inner.add_account().await })
            .await
            .map_err(|e| Error::NotFound(e.to_string()))?
    }

    pub async fn remove_account(&self, account_id: i64) -> Result<()> {
        let inner = self.inner.clone();
        inner
            .handle
            .clone()
            .spawn(async move { inner.remove_account(account_id) })
            .await
            .map_err(|e| Error::NotFound(e.to_string()))?
    }

    /// Download to the attachment cache (with Mark-of-the-Web); returns the path.
    pub async fn download_attachment(&self, attachment_id: i64) -> Result<PathBuf> {
        let inner = self.inner.clone();
        inner
            .handle
            .clone()
            .spawn(async move { inner.download_attachment(attachment_id).await })
            .await
            .map_err(|e| Error::NotFound(e.to_string()))?
    }
}

pub(crate) const ACCOUNT_COLORS: &[&str] = &[
    "#4f8cff", "#e5484d", "#30a46c", "#f5a524", "#8e4ec6", "#12a594", "#e93d82", "#6e56cf",
];

impl Inner {
    pub fn fg(&self) -> MutexGuard<'_, Connection> {
        self.fg.lock().expect("fg connection poisoned")
    }

    pub fn account_wake(&self, account_id: i64) -> Arc<Notify> {
        self.account_wake
            .lock()
            .expect("lock")
            .entry(account_id)
            .or_default()
            .clone()
    }

    pub fn spawn_account(self: &Arc<Self>, account_id: i64) {
        let task = self
            .handle
            .spawn(syncer::run_account(self.clone(), account_id));
        self.tasks.lock().expect("lock").insert(account_id, task);
    }

    pub fn set_state(&self, account_id: i64, state: AccountSyncState) {
        let changed = self.states.lock().expect("lock").insert(account_id, state) != Some(state);
        if changed {
            self.notifier.status_changed(self.status());
        }
    }

    pub fn status(&self) -> SyncStatus {
        let states = self.states.lock().expect("lock").clone();
        let conn = self.fg();
        let q = || -> Result<SyncStatus> {
            let rows = conn
                .prepare_cached(
                    "SELECT a.id, s.last_sync_at, s.sync_from, \
                     s.account_id IS NOT NULL AND s.backfill_cursor IS NOT 'done', \
                     s.messages_total \
                     FROM accounts a LEFT JOIN sync_state s ON s.account_id = a.id \
                     ORDER BY a.sort_order, a.id",
                )?
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, bool>(3)?,
                        r.get::<_, Option<i64>>(4)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let now = self.clock.now_ms();
            let mut accounts = Vec::with_capacity(rows.len());
            for (id, last, from, running, total) in rows {
                let percent = match (running, from) {
                    (false, _) => None,
                    // A window: how far back (newest first) the stored mail reaches.
                    (true, Some(from)) => {
                        let oldest: Option<i64> = conn
                            .prepare_cached(
                                "SELECT min(internal_date) FROM messages \
                                 WHERE account_id = ?1 AND internal_date >= ?2",
                            )?
                            .query_row(params![id, from], |r| r.get(0))?;
                        let span = (now - from).max(1);
                        Some(oldest.map_or(0, |o| (now - o).clamp(0, span) * 100 / span))
                    }
                    // All mail: stored against Gmail's total.
                    (true, None) => match total.filter(|t| *t > 0) {
                        None => None,
                        Some(t) => {
                            let stored: i64 = conn
                                .prepare_cached(
                                    "SELECT count(*) FROM messages WHERE account_id = ?1",
                                )?
                                .query_row([id], |r| r.get(0))?;
                            Some(stored * 100 / t)
                        }
                    },
                };
                // Wider than the default window (30 days of slack: the first
                // sync of a big mailbox can run for days).
                let default_from = self
                    .cfg
                    .sync_window
                    .map(|w| now - w.as_millis() as i64 - 30 * 86_400_000);
                let older = match (from, default_from) {
                    (_, None) => false,
                    (None, Some(_)) => true,
                    (Some(f), Some(d)) => f < d,
                };
                accounts.push(AccountSync {
                    account_id: id,
                    state: states.get(&id).copied().unwrap_or(AccountSyncState::Idle),
                    last_sync_at: last,
                    sync_from: from,
                    backfill: percent.map(|p| Backfill {
                        percent: p.clamp(0, 99) as u8,
                        older,
                    }),
                });
            }
            let pending: u32 = conn
                .prepare_cached("SELECT count(*) FROM action_queue WHERE failed = 0")?
                .query_row([], |r| r.get(0))?;
            let failed_actions = conn
                .prepare_cached(
                    "SELECT id, thread_id, payload_json, COALESCE(last_error, '') \
                     FROM action_queue WHERE failed = 1 ORDER BY id",
                )?
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .filter_map(|(id, thread_id, json, error)| {
                    let p: actions::Payload = serde_json::from_str(&json).ok()?;
                    Some(FailedAction {
                        id,
                        thread_id,
                        kind: p.kind,
                        error,
                    })
                })
                .collect();
            Ok(SyncStatus {
                accounts,
                pending_actions: pending,
                failed_actions,
            })
        };
        q().unwrap_or_default()
    }

    /// API handle for an account (by local id).
    pub fn api_for(
        &self,
        conn: &Connection,
        account_id: i64,
    ) -> std::result::Result<Arc<dyn GmailApi>, ApiError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| ApiError::Auth("no Gmail provider configured".into()))?;
        let email: String = conn
            .prepare_cached("SELECT email FROM accounts WHERE id = ?1")
            .and_then(|mut s| s.query_row([account_id], |r| r.get(0)))
            .map_err(|e| ApiError::Other(e.to_string()))?;
        provider.api(&email)
    }

    pub fn request_restore(&self, account_id: i64, thread_id: i64) {
        self.restore
            .lock()
            .expect("lock")
            .entry(account_id)
            .or_default()
            .insert(thread_id);
        self.account_wake(account_id).notify_one();
    }

    async fn add_account(self: Arc<Self>) -> Result<Account> {
        let provider = self
            .provider
            .clone()
            .ok_or_else(|| ApiError::Auth("no Gmail provider configured".into()))?;
        let signed = provider.sign_in().await?;
        let account = {
            let conn = self.fg();
            let existing = conn
                .prepare_cached("SELECT id FROM accounts WHERE email = ?1")?
                .query_row([&signed.email], |r| r.get::<_, i64>(0))
                .optional()?;
            let (id, fresh) = match existing {
                Some(id) => (id, false),
                None => {
                    let n: i64 =
                        conn.query_row("SELECT count(*) FROM accounts", [], |r| r.get(0))?;
                    let color = ACCOUNT_COLORS[n as usize % ACCOUNT_COLORS.len()];
                    let id = tidemail_db::write::insert_account(
                        &conn,
                        &tidemail_db::write::NewAccount {
                            email: &signed.email,
                            display_name: signed.display_name.as_deref(),
                            color,
                            sort_order: n,
                        },
                    )?;
                    (id, true)
                }
            };
            let account = tidemail_db::read::list_accounts(&conn)?
                .into_iter()
                .find(|a| a.id == id)
                .ok_or_else(|| Error::NotFound("account".into()))?;
            (account, fresh)
        };
        let (account, fresh) = account;
        if fresh || !self.tasks.lock().expect("lock").contains_key(&account.id) {
            self.spawn_account(account.id);
        }
        self.notifier.status_changed(self.status());
        Ok(account)
    }

    fn remove_account(&self, account_id: i64) -> Result<()> {
        if let Some(t) = self.tasks.lock().expect("lock").remove(&account_id) {
            t.abort();
        }
        let email: Option<String> = {
            let mut conn = self.fg();
            let email = conn
                .prepare_cached("SELECT email FROM accounts WHERE id = ?1")?
                .query_row([account_id], |r| r.get(0))
                .optional()?;
            let tx = conn.transaction()?;
            store::delete_account_fts(&tx, account_id)?;
            tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
            tx.commit()?;
            email
        };
        self.states.lock().expect("lock").remove(&account_id);
        if let (Some(p), Some(email)) = (&self.provider, email) {
            p.sign_out(&email)?;
        }
        self.notifier.data_changed(vec![], vec![]);
        self.notifier.status_changed(self.status());
        Ok(())
    }

    async fn download_attachment(&self, attachment_id: i64) -> Result<PathBuf> {
        struct Row {
            account_id: i64,
            message_id: i64,
            msg_gmail: String,
            att_gmail: Option<String>,
            filename: String,
            local: Option<String>,
            inline: Option<Vec<u8>>,
        }
        let row = {
            let conn = self.fg();
            let row = conn
                .prepare_cached(
                    "SELECT m.account_id, m.id, m.gmail_id, a.gmail_attachment_id, a.filename,                      a.local_path, b.data FROM attachments a JOIN messages m ON m.id = a.message_id                      LEFT JOIN inline_blobs b ON b.attachment_id = a.id WHERE a.id = ?1",
                )?
                .query_row([attachment_id], |r| {
                    Ok(Row {
                        account_id: r.get(0)?,
                        message_id: r.get(1)?,
                        msg_gmail: r.get(2)?,
                        att_gmail: r.get(3)?,
                        filename: r.get(4)?,
                        local: r.get(5)?,
                        inline: r.get(6)?,
                    })
                })
                .optional()?
                .ok_or_else(|| Error::NotFound(format!("attachment {attachment_id}")))?;
            row
        };
        let Row {
            account_id,
            message_id,
            msg_gmail,
            att_gmail,
            filename,
            local,
            inline,
        } = row;
        if let Some(p) = local.map(PathBuf::from).filter(|p| p.exists()) {
            return Ok(p);
        }
        let data = match (inline, att_gmail) {
            (Some(d), _) => d,
            (None, Some(att)) => {
                let api = {
                    let conn = self.fg();
                    self.api_for(&conn, account_id)?
                };
                api.get_attachment(&msg_gmail, &att).await?
            }
            (None, None) => {
                return Err(Error::NotFound(format!("attachment {attachment_id} data")))
            }
        };
        let base = self.db_path.parent().map(PathBuf::from).unwrap_or_default();
        let path =
            tidemail_render::save_attachment_in(&base, account_id, message_id, &filename, &data)?;
        self.fg()
            .prepare_cached("UPDATE attachments SET local_path = ?2 WHERE id = ?1")?
            .execute(params![attachment_id, path.to_string_lossy()])?;
        Ok(path)
    }
}
