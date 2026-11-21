//! Per-account sync loop: backfill (metadata pages, then bodies), live
//! `history.list` polling, full id diff, restore after failed actions.
//! Also the body prefetch worker.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tidemail_db::rusqlite::{params, Connection, OptionalExtension};
use tidemail_db::types::AccountSyncState;
use tidemail_db::write;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::actions::reapply_pending;
use crate::api::{ApiError, Format, GmailApi, RemoteMessage};
use crate::store::{self, INLINE_MAX};
use crate::{Error, Inner, Result};

type Db = Arc<Mutex<Connection>>;

/// Labels compared by the full id diff to find messages whose labels changed.
const DIFF_LABELS: &[&str] = &["INBOX", "UNREAD", "STARRED", "TRASH", "SPAM"];
const BATCH: usize = 50;

fn lock(db: &Db) -> std::sync::MutexGuard<'_, Connection> {
    db.lock().expect("db connection poisoned")
}

enum Step {
    More,
    Idle,
}

/// Threads and label keys touched by a batch; reconciled and announced at the end.
#[derive(Default)]
struct Affected {
    threads: BTreeSet<i64>,
    keys: BTreeSet<String>,
}

impl Affected {
    fn touch(&mut self, conn: &Connection, thread_id: i64) -> Result<()> {
        if self.threads.insert(thread_id) {
            self.keys.extend(store::thread_label_keys(conn, thread_id)?);
        }
        Ok(())
    }

    /// Re-apply pending actions on top of the remote state, recompute.
    fn reconcile(&mut self, conn: &Connection) -> Result<()> {
        for &t in &self.threads {
            reapply_pending(conn, t)?;
            write::recompute_thread(conn, t)?;
            self.keys.extend(store::thread_label_keys(conn, t)?);
        }
        Ok(())
    }
}

struct AccountLoop {
    inner: Arc<Inner>,
    id: i64,
    db: Db,
    wake: Arc<tokio::sync::Notify>,
    initialized: bool,
    last_poll: Option<Instant>,
}

pub async fn run_account(inner: Arc<Inner>, account_id: i64) {
    let db = match tidemail_db::open(&inner.db_path) {
        Ok(c) => Arc::new(Mutex::new(c)),
        Err(e) => {
            eprintln!("tidemail-sync: account {account_id}: cannot open db: {e}");
            return;
        }
    };
    let mut a = AccountLoop {
        wake: inner.account_wake(account_id),
        inner: inner.clone(),
        id: account_id,
        db,
        initialized: false,
        last_poll: None,
    };
    let mut backoff = Duration::from_secs(5);
    loop {
        match a.step().await {
            Ok(Step::More) => {
                inner.set_state(account_id, AccountSyncState::Syncing);
                backoff = Duration::from_secs(5);
                tokio::task::yield_now().await;
            }
            Ok(Step::Idle) => {
                inner.set_state(account_id, AccountSyncState::Idle);
                backoff = Duration::from_secs(5);
                let next = a.last_poll.map_or(Duration::ZERO, |t| {
                    inner.cfg.poll_interval.saturating_sub(t.elapsed())
                });
                tokio::select! {
                    _ = tokio::time::sleep(next) => {}
                    _ = a.wake.notified() => {}
                }
            }
            Err(e) => {
                let (state, wait) = match &e {
                    Error::Api(ApiError::Offline(_)) => (AccountSyncState::Offline, backoff),
                    Error::Api(ApiError::Auth(_)) => (
                        AccountSyncState::AuthError,
                        backoff.max(Duration::from_secs(60)),
                    ),
                    _ => (AccountSyncState::Idle, backoff),
                };
                eprintln!("tidemail-sync: account {account_id}: {e}");
                inner.set_state(account_id, state);
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = a.wake.notified() => {}
                }
                backoff = (backoff * 2).min(Duration::from_secs(300));
            }
        }
    }
}

impl AccountLoop {
    fn cfg(&self) -> &crate::Config {
        &self.inner.cfg
    }

    async fn pace(&self) {
        tokio::time::sleep(self.cfg().batch_gap).await;
    }

    async fn step(&mut self) -> Result<Step> {
        let api = {
            let conn = lock(&self.db);
            self.inner.api_for(&conn, self.id)?
        };
        if !self.initialized {
            self.init(&*api).await?;
            self.initialized = true;
        }
        self.restore(&*api).await?;
        if self
            .last_poll
            .is_none_or(|t| t.elapsed() >= self.cfg().poll_interval)
        {
            self.poll(&*api).await?;
            self.last_poll = Some(Instant::now());
        }
        // Bodies of recent mail arrive alongside the metadata backfill, not
        // after it (a big mailbox takes hours of metadata).
        let listed = self.backfill_page(&*api).await?;
        let bodies = self.body_batch(&*api).await?;
        if listed || bodies || self.resanitize_batch(&*api).await? {
            return Ok(Step::More);
        }
        Ok(Step::Idle)
    }

    async fn init(&self, api: &dyn GmailApi) -> Result<()> {
        let has_history: bool = {
            let conn = lock(&self.db);
            // A new account lists only the default window of recent mail.
            let from = self
                .cfg()
                .sync_window
                .map(|w| self.inner.clock.now_ms() - w.as_millis() as i64);
            conn.execute(
                "INSERT OR IGNORE INTO sync_state (account_id, sync_from) VALUES (?1, ?2)",
                params![self.id, from],
            )?;
            conn.query_row(
                "SELECT history_id IS NOT NULL FROM sync_state WHERE account_id = ?1",
                [self.id],
                |r| r.get(0),
            )?
        };
        // Read before listing, so changes during the backfill are not missed.
        // The total is refreshed on every start (status line progress).
        let profile = api.get_profile().await?;
        if !has_history {
            self.set_history(&profile.history_id)?;
        }
        lock(&self.db).execute(
            "UPDATE sync_state SET messages_total = ?2 WHERE account_id = ?1",
            params![self.id, profile.messages_total],
        )?;
        self.sync_labels(api).await
    }

    async fn sync_labels(&self, api: &dyn GmailApi) -> Result<()> {
        let labels = api.list_labels().await?;
        store::sync_labels(&lock(&self.db), self.id, &labels)
    }

    /// Oldest mail to list (epoch ms); `None`: all mail.
    fn sync_from(&self) -> Result<Option<i64>> {
        Ok(lock(&self.db).query_row(
            "SELECT sync_from FROM sync_state WHERE account_id = ?1",
            [self.id],
            |r| r.get(0),
        )?)
    }

    fn set_history(&self, history_id: &str) -> Result<()> {
        lock(&self.db).execute(
            "UPDATE sync_state SET history_id = ?2 WHERE account_id = ?1",
            params![self.id, history_id],
        )?;
        Ok(())
    }

    fn mark_synced(&self) -> Result<()> {
        lock(&self.db).execute(
            "UPDATE sync_state SET last_sync_at = ?2 WHERE account_id = ?1",
            params![self.id, self.inner.clock.now_ms()],
        )?;
        self.inner.notifier.status_changed(self.inner.status());
        Ok(())
    }

    // ---- backfill ----

    /// One page of `messages.list` plus metadata for its new ids. True if work was done.
    async fn backfill_page(&self, api: &dyn GmailApi) -> Result<bool> {
        let cursor: Option<String> = lock(&self.db).query_row(
            "SELECT backfill_cursor FROM sync_state WHERE account_id = ?1",
            [self.id],
            |r| r.get(0),
        )?;
        if cursor.as_deref() == Some("done") {
            return Ok(false);
        }
        let token = cursor
            .as_deref()
            .and_then(|c| c.strip_prefix("meta:"))
            .filter(|t| !t.is_empty());
        let query = self.sync_from()?.map(window_query);
        let page = api.list_message_ids(None, query.as_deref(), token).await?;
        let new_ids: Vec<String> = {
            let conn = lock(&self.db);
            let mut st = conn
                .prepare_cached("SELECT 1 FROM messages WHERE account_id = ?1 AND gmail_id = ?2")?;
            let mut out = Vec::new();
            for m in &page.ids {
                if !st.exists(params![self.id, m.id])? {
                    out.push(m.id.clone());
                }
            }
            out
        };
        self.apply_remote(api, &new_ids, &[]).await?;

        let next = match &page.next_page_token {
            Some(t) => format!("meta:{t}"),
            None => "done".to_owned(),
        };
        let conn = lock(&self.db);
        conn.execute(
            "UPDATE sync_state SET backfill_cursor = ?2 WHERE account_id = ?1",
            params![self.id, next],
        )?;
        // Every page: the body pass must not fetch old bodies in between.
        self.apply_body_policy(&conn)?;
        Ok(true)
    }

    /// Big accounts: only the last two years get bodies; older are deferred.
    fn apply_body_policy(&self, conn: &Connection) -> Result<()> {
        let total: i64 = conn.query_row(
            "SELECT count(*) FROM messages WHERE account_id = ?1",
            [self.id],
            |r| r.get(0),
        )?;
        if total as u64 > self.cfg().body_policy_threshold {
            let cutoff = self.inner.clock.now_ms() - self.cfg().body_window.as_millis() as i64;
            conn.execute(
                "UPDATE messages SET body_state = 2 WHERE account_id = ?1 AND body_state = 0 \
                 AND internal_date < ?2",
                params![self.id, cutoff],
            )?;
        }
        Ok(())
    }

    /// Fetch one batch of missing bodies, newest first. True if work was done.
    async fn body_batch(&self, api: &dyn GmailApi) -> Result<bool> {
        let rows: Vec<(i64, String)> = {
            let conn = lock(&self.db);
            let mut st = conn.prepare_cached(
                "SELECT id, gmail_id FROM messages WHERE account_id = ?1 AND body_state = 0 \
                 ORDER BY internal_date DESC LIMIT ?2",
            )?;
            let rows = st.query_map(params![self.id, BATCH as i64], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        if rows.is_empty() {
            return Ok(false);
        }
        let threads = store_bodies(&self.db, self.id, api, &rows).await?;
        self.inner.notifier.data_changed(threads, vec![]);
        self.pace().await;
        Ok(true)
    }

    /// Re-sanitize bodies stored by an older `SANITIZER_VERSION`: refetch the
    /// raw HTML (format=full), newest first, after backfill and body passes
    /// are done. Messages Gmail no longer has are deleted by `store_bodies`.
    async fn resanitize_batch(&self, api: &dyn GmailApi) -> Result<bool> {
        let rows: Vec<(i64, String)> = {
            let conn = lock(&self.db);
            let mut st = conn.prepare_cached(
                "SELECT m.id, m.gmail_id FROM bodies b JOIN messages m ON m.id = b.message_id                  WHERE b.sanitizer_version < ?1 AND m.account_id = ?2                  ORDER BY m.internal_date DESC LIMIT ?3",
            )?;
            let rows = st.query_map(
                params![tidemail_render::SANITIZER_VERSION, self.id, BATCH as i64],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        if rows.is_empty() {
            return Ok(false);
        }
        let threads = store_bodies(&self.db, self.id, api, &rows).await?;
        self.inner.notifier.data_changed(threads, vec![]);
        self.pace().await;
        Ok(true)
    }

    // ---- remote changes ----

    /// Fetch metadata for `ids` and store it; `deleted` and ids Gmail no
    /// longer knows are removed. Pending actions are re-applied on top.
    async fn apply_remote(
        &self,
        api: &dyn GmailApi,
        ids: &[String],
        deleted: &[String],
    ) -> Result<()> {
        let mut affected = Affected::default();
        for chunk in ids.chunks(BATCH) {
            let fetched = api.get_messages(chunk, Format::Metadata).await?;
            let mut labels = store::label_map(&lock(&self.db), self.id)?;
            if fetched
                .iter()
                .any(|m| m.label_ids.iter().any(|l| !labels.contains_key(l)))
            {
                self.sync_labels(api).await?;
                labels = store::label_map(&lock(&self.db), self.id)?;
            }
            let known: HashSet<&str> = fetched.iter().map(|m| m.id.as_str()).collect();
            let gone: Vec<&String> = chunk
                .iter()
                .filter(|id| !known.contains(id.as_str()))
                .collect();
            {
                let mut conn = lock(&self.db);
                let tx = conn.transaction()?;
                for m in &fetched {
                    if let Some(t) = thread_by_gmail(&tx, self.id, &m.thread_id)? {
                        affected.touch(&tx, t)?;
                    }
                    let t = store::ingest_meta(&tx, self.id, &labels, m)?;
                    affected.touch(&tx, t)?;
                }
                for g in gone {
                    self.delete_local(&tx, g, &mut affected)?;
                }
                affected.reconcile(&tx)?;
                tx.commit()?;
            }
            self.pace().await;
        }
        if !deleted.is_empty() {
            let mut conn = lock(&self.db);
            let tx = conn.transaction()?;
            for g in deleted {
                self.delete_local(&tx, g, &mut affected)?;
            }
            affected.reconcile(&tx)?;
            tx.commit()?;
        }
        if !affected.threads.is_empty() {
            self.inner.notifier.data_changed(
                affected.threads.into_iter().collect(),
                affected.keys.into_iter().collect(),
            );
        }
        Ok(())
    }

    fn delete_local(
        &self,
        conn: &Connection,
        gmail_id: &str,
        affected: &mut Affected,
    ) -> Result<()> {
        let thread: Option<i64> = conn
            .prepare_cached(
                "SELECT thread_id FROM messages WHERE account_id = ?1 AND gmail_id = ?2",
            )?
            .query_row(params![self.id, gmail_id], |r| r.get(0))
            .optional()?;
        if let Some(t) = thread {
            affected.touch(conn, t)?;
            store::delete_message(conn, self.id, gmail_id)?;
        }
        Ok(())
    }

    /// Re-fetch Gmail state for threads whose actions failed or were dismissed.
    async fn restore(&self, api: &dyn GmailApi) -> Result<()> {
        let threads: Vec<i64> = match self.inner.restore.lock().expect("lock").remove(&self.id) {
            Some(t) => t.into_iter().collect(),
            None => return Ok(()),
        };
        let ids: Vec<String> = {
            let conn = lock(&self.db);
            let mut out = Vec::new();
            for t in &threads {
                let mut st =
                    conn.prepare_cached("SELECT gmail_id FROM messages WHERE thread_id = ?1")?;
                let rows = st.query_map([t], |r| r.get::<_, String>(0))?;
                for r in rows {
                    out.push(r?);
                }
            }
            out
        };
        if let Err(e) = self.apply_remote(api, &ids, &[]).await {
            self.inner
                .restore
                .lock()
                .expect("lock")
                .entry(self.id)
                .or_default()
                .extend(threads);
            return Err(e);
        }
        Ok(())
    }

    /// `history.list` since the stored id; 404 means a full id diff.
    async fn poll(&self, api: &dyn GmailApi) -> Result<()> {
        let start: Option<String> = lock(&self.db).query_row(
            "SELECT history_id FROM sync_state WHERE account_id = ?1",
            [self.id],
            |r| r.get(0),
        )?;
        let Some(start) = start else { return Ok(()) };
        let mut changed = BTreeSet::new();
        let mut deleted = BTreeSet::new();
        let newest;
        let mut token: Option<String> = None;
        loop {
            match api.list_history(&start, token.as_deref()).await {
                Err(ApiError::HistoryExpired) => return self.full_diff(api).await,
                Err(e) => return Err(e.into()),
                Ok(p) => {
                    changed.extend(p.added);
                    changed.extend(p.label_changed);
                    deleted.extend(p.deleted);
                    token = p.next_page_token;
                    if token.is_none() {
                        newest = p.history_id;
                        break;
                    }
                }
            }
        }
        let fetch: Vec<String> = changed.difference(&deleted).cloned().collect();
        let deleted: Vec<String> = deleted.into_iter().collect();
        self.apply_remote(api, &fetch, &deleted).await?;
        self.set_history(&newest)?;
        self.mark_synced()
    }

    /// History expired: list all ids, fetch new, delete removed, refetch
    /// messages whose label membership differs for the tracked labels.
    async fn full_diff(&self, api: &dyn GmailApi) -> Result<()> {
        self.inner.set_state(self.id, AccountSyncState::Syncing);
        let history_id = api.get_profile().await?.history_id;
        let from = self.sync_from()?;
        let query = from.map(window_query);
        let remote = list_all(api, None, query.as_deref()).await?;
        // Stored mail older than the window is not in `remote`; it is not
        // deleted. A day of margin covers how Gmail rounds `after:`.
        let keep_before = from.map_or(i64::MIN, |f| f + 86_400_000);
        let (local, in_window): (HashSet<String>, HashSet<String>) = {
            let conn = lock(&self.db);
            let mut st = conn.prepare_cached(
                "SELECT gmail_id, internal_date FROM messages WHERE account_id = ?1",
            )?;
            let rows = st.query_map([self.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            let (mut all, mut window) = (HashSet::new(), HashSet::new());
            for row in rows {
                let (id, date) = row?;
                if date > keep_before {
                    window.insert(id.clone());
                }
                all.insert(id);
            }
            (all, window)
        };
        let mut fetch: BTreeSet<String> = remote.difference(&local).cloned().collect();
        let removed: Vec<String> = in_window.difference(&remote).cloned().collect();
        for label in DIFF_LABELS {
            let remote_l = list_all(api, Some(label), query.as_deref()).await?;
            let local_l: HashSet<String> = {
                let conn = lock(&self.db);
                let mut st = conn.prepare_cached(
                    "SELECT m.gmail_id FROM messages m JOIN message_labels ml ON ml.message_id = m.id \
                     JOIN labels l ON l.id = ml.label_id WHERE m.account_id = ?1 AND l.gmail_id = ?2",
                )?;
                let rows = st.query_map(params![self.id, label], |r| r.get(0))?;
                rows.collect::<std::result::Result<_, _>>()?
            };
            fetch.extend(
                remote_l
                    .symmetric_difference(&local_l)
                    .filter(|id| remote.contains(*id))
                    .cloned(),
            );
        }
        let fetch: Vec<String> = fetch.into_iter().collect();
        self.apply_remote(api, &fetch, &removed).await?;
        self.set_history(&history_id)?;
        self.mark_synced()
    }
}

fn thread_by_gmail(conn: &Connection, account_id: i64, gmail_thread: &str) -> Result<Option<i64>> {
    Ok(conn
        .prepare_cached("SELECT id FROM threads WHERE account_id = ?1 AND gmail_id = ?2")?
        .query_row(params![account_id, gmail_thread], |r| r.get(0))
        .optional()?)
}

/// Gmail search for mail newer than `from_ms`.
fn window_query(from_ms: i64) -> String {
    format!("after:{}", from_ms.div_euclid(1000))
}

async fn list_all(
    api: &dyn GmailApi,
    label: Option<&str>,
    query: Option<&str>,
) -> Result<HashSet<String>> {
    let mut out = HashSet::new();
    let mut token: Option<String> = None;
    loop {
        let page = api.list_message_ids(label, query, token.as_deref()).await?;
        out.extend(page.ids.into_iter().map(|m| m.id));
        token = page.next_page_token;
        if token.is_none() {
            return Ok(out);
        }
    }
}

/// Fetch `Format::Full` for local messages (`(row id, gmail id)`) and store
/// bodies. Messages Gmail no longer has are deleted. Returns touched thread ids.
async fn store_bodies(
    db: &Db,
    account_id: i64,
    api: &dyn GmailApi,
    rows: &[(i64, String)],
) -> Result<Vec<i64>> {
    let by_gmail: HashMap<&str, i64> = rows.iter().map(|(id, g)| (g.as_str(), *id)).collect();
    let ids: Vec<String> = rows.iter().map(|(_, g)| g.clone()).collect();
    let mut threads = BTreeSet::new();
    for chunk in ids.chunks(BATCH) {
        let fetched = api.get_messages(chunk, Format::Full).await?;
        // Inline cid: parts under 200 KB; bytes come inline or via get_attachment.
        let mut blobs: Vec<HashMap<String, Vec<u8>>> = Vec::with_capacity(fetched.len());
        for m in &fetched {
            blobs.push(inline_blobs(api, m).await);
        }
        let known: HashSet<&str> = fetched.iter().map(|m| m.id.as_str()).collect();
        let mut conn = lock(db);
        let tx = conn.transaction()?;
        for (m, b) in fetched.iter().zip(&blobs) {
            let Some(&mid) = by_gmail.get(m.id.as_str()) else {
                continue;
            };
            store::ingest_full(&tx, mid, m, b)?;
            let t: i64 =
                tx.query_row("SELECT thread_id FROM messages WHERE id = ?1", [mid], |r| {
                    r.get(0)
                })?;
            write::recompute_thread(&tx, t)?;
            threads.insert(t);
        }
        for g in chunk.iter().filter(|g| !known.contains(g.as_str())) {
            let t: Option<i64> = tx
                .query_row(
                    "SELECT thread_id FROM messages WHERE account_id = ?1 AND gmail_id = ?2",
                    params![account_id, g],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(t) = t {
                store::delete_message(&tx, account_id, g)?;
                write::recompute_thread(&tx, t)?;
                threads.insert(t);
            }
        }
        tx.commit()?;
    }
    Ok(threads.into_iter().collect())
}

async fn inline_blobs(api: &dyn GmailApi, m: &RemoteMessage) -> HashMap<String, Vec<u8>> {
    let mut out = HashMap::new();
    let Some(body) = &m.body else { return out };
    for a in body
        .attachments
        .iter()
        .filter(|a| a.is_inline && a.content_id.is_some() && a.size < INLINE_MAX)
    {
        let data = match (&a.data, &a.attachment_id) {
            (Some(d), _) => Some(d.clone()),
            // One missing image must not fail the batch; the reader shows a broken image.
            (None, Some(att)) => api.get_attachment(&m.id, att).await.ok(),
            (None, None) => None,
        };
        if let Some(d) = data {
            out.insert(a.part_id.clone(), d);
        }
    }
    out
}

/// Prefetch worker: bodies for opened-next messages, bounded queue.
pub async fn prefetch_loop(inner: Arc<Inner>, mut rx: mpsc::Receiver<i64>) {
    let db: Db = match tidemail_db::open(&inner.db_path) {
        Ok(c) => Arc::new(Mutex::new(c)),
        Err(e) => {
            eprintln!("tidemail-sync: prefetch cannot open db: {e}");
            return;
        }
    };
    while let Some(first) = rx.recv().await {
        let mut ids = vec![first];
        while ids.len() < BATCH {
            match rx.try_recv() {
                Ok(id) => ids.push(id),
                Err(_) => break,
            }
        }
        if let Err(e) = prefetch(&inner, &db, &ids).await {
            eprintln!("tidemail-sync: prefetch: {e}");
        }
    }
}

async fn prefetch(inner: &Inner, db: &Db, ids: &[i64]) -> Result<()> {
    let mut per_account: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    {
        let conn = lock(db);
        let mut st = conn.prepare_cached(
            "SELECT account_id, gmail_id FROM messages WHERE id = ?1 AND body_state != 1",
        )?;
        for id in ids {
            if let Some((a, g)) = st
                .query_row([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .optional()?
            {
                per_account.entry(a).or_default().push((*id, g));
            }
        }
    }
    for (account_id, rows) in per_account {
        let api = {
            let conn = lock(db);
            inner.api_for(&conn, account_id)?
        };
        let threads = store_bodies(db, account_id, &*api, &rows).await?;
        inner.notifier.data_changed(threads, vec![]);
    }
    Ok(())
}
