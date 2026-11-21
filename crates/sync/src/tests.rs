//! Engine tests against the fake Gmail. Tokio time is paused, so seconds of
//! engine time cost nothing. The DB is a temp file (several connections
//! need a shared WAL file; `:memory:` cannot be shared).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tidemail_db::rusqlite::Connection;
use tidemail_db::types::{AccountSyncState, ActionKind, ActionRequest, SyncStatus};
use tidemail_db::write::{self, NewAccount};
use tokio::runtime::Handle;

use crate::api::{ApiError, RemoteAttachment};
use crate::fake::{label_set, FakeGmail, FakeMsg, FakeProvider};
use crate::{Config, Engine, Notifier};

#[derive(Default)]
struct Rec {
    data: Mutex<Vec<(Vec<i64>, Vec<String>)>>,
    status: Mutex<Vec<SyncStatus>>,
}

impl Notifier for Rec {
    fn data_changed(&self, t: Vec<i64>, k: Vec<String>) {
        self.data.lock().unwrap().push((t, k));
    }
    fn status_changed(&self, s: SyncStatus) {
        self.status.lock().unwrap().push(s);
    }
}

static N: AtomicU32 = AtomicU32::new(0);

struct H {
    dir: PathBuf,
    path: PathBuf,
    engine: Engine,
    gmail: Arc<FakeGmail>,
    rec: Arc<Rec>,
    ro: Connection,
}

impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn setup(cfg: Config, with_account: bool, gmail: Arc<FakeGmail>) -> H {
    let dir = std::env::temp_dir().join(format!(
        "tidemail-sync-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mail.db");
    {
        let conn = tidemail_db::open(&path).unwrap();
        if with_account {
            write::insert_account(
                &conn,
                &NewAccount {
                    email: "me@example.com",
                    display_name: None,
                    color: "#000000",
                    sort_order: 0,
                },
            )
            .unwrap();
        }
    }
    let rec = Arc::new(Rec::default());
    let provider = Arc::new(FakeProvider {
        api: gmail.clone(),
        email: "me@example.com".into(),
    });
    let engine = Engine::start_on(
        Handle::current(),
        path.clone(),
        rec.clone(),
        Some(provider),
        cfg,
    );
    let ro = tidemail_db::open(&path).unwrap();
    H {
        dir,
        path,
        engine,
        gmail,
        rec,
        ro,
    }
}

fn cfg() -> Config {
    Config {
        batch_gap: Duration::ZERO,
        sync_window: None,
        ..Config::default()
    }
}

async fn until(mut f: impl FnMut() -> bool) {
    for _ in 0..4000 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not reached");
}

/// Like `until`, for waits of minutes (backoff).
async fn until_long(mut f: impl FnMut() -> bool) {
    for _ in 0..2000 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    panic!("condition not reached");
}

async fn secs(n: u64) {
    tokio::time::sleep(Duration::from_secs(n)).await;
}

fn count(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn msg(i: i64, thread: i64, labels: &[&str], age_min: i64, base: i64) -> FakeMsg {
    FakeMsg {
        id: format!("m{i}"),
        thread_id: format!("t{thread}"),
        labels: label_set(labels),
        date: base - age_min * 60_000,
        from: format!("\"Sender {i}\" <s{i}@example.com>"),
        subject: format!("Subject {i}"),
        html: Some(format!(
            "<p>hello pineapple{i}</p><script>evil()</script><img src=\"cid:logo\">"
        )),
        text: None,
        attachments: vec![],
    }
}

/// 130 messages in 44 threads, newest first by index.
fn big_mailbox(g: &FakeGmail) {
    let base = tidemail_db::now_ms();
    for i in 0..130 {
        let mut labels = vec!["INBOX"];
        if i % 5 == 0 {
            labels.push("UNREAD");
        }
        if i % 7 == 0 {
            labels.push("STARRED");
        }
        let mut m = msg(i, i / 3, &labels, i, base);
        if i % 10 == 0 {
            m.attachments.push(RemoteAttachment {
                part_id: "1".into(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                size: 1000,
                content_id: Some("logo".into()),
                is_inline: true,
                data: Some(vec![1; 1000]),
                ..Default::default()
            });
        }
        if i == 3 {
            m.attachments.push(RemoteAttachment {
                part_id: "2".into(),
                attachment_id: Some("att3".into()),
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                size: 5000,
                ..Default::default()
            });
        }
        if i == 4 {
            m.attachments.push(RemoteAttachment {
                part_id: "1".into(),
                attachment_id: Some("att4".into()),
                filename: "sig.png".into(),
                mime_type: "image/png".into(),
                size: 100,
                content_id: Some("logo".into()),
                is_inline: true,
                data: None,
            });
        }
        g.add(m);
    }
}

fn backfill_done(c: &Connection) -> bool {
    c.query_row(
        "SELECT backfill_cursor = 'done' AND NOT EXISTS (SELECT 1 FROM messages WHERE body_state = 0) \
         FROM sync_state",
        [],
        |r| r.get(0),
    )
    .unwrap_or(false)
}

fn fts_hits(c: &Connection, q: &str) -> i64 {
    c.query_row(
        "SELECT count(*) FROM messages_fts WHERE messages_fts MATCH ?1",
        [q],
        |r| r.get(0),
    )
    .unwrap()
}

fn thread_id(c: &Connection, gmail: &str) -> i64 {
    c.query_row("SELECT id FROM threads WHERE gmail_id = ?1", [gmail], |r| {
        r.get(0)
    })
    .unwrap()
}

fn in_label(c: &Connection, thread: i64, key: &str) -> bool {
    count(
        c,
        &format!(
            "SELECT count(*) FROM thread_labels WHERE thread_id = {thread} AND label_key = '{key}'"
        ),
    ) > 0
}

async fn backfilled() -> H {
    let g = FakeGmail::new();
    big_mailbox(&g);
    let h = setup(cfg(), true, g);
    until(|| backfill_done(&h.ro)).await;
    h
}

// ---- backfill ----

#[tokio::test(start_paused = true)]
async fn old_sanitizer_bodies_are_refetched_and_resanitized() {
    let h = backfilled().await;
    let rw = tidemail_db::open(&h.path).unwrap();
    rw.execute(
        "UPDATE bodies SET sanitizer_version = 1, html_sanitized = '<p>old</p>'          WHERE message_id = (SELECT id FROM messages WHERE gmail_id = 'm1')",
        [],
    )
    .unwrap();
    let current = tidemail_render::SANITIZER_VERSION as i64;
    until_long(|| {
        count(
            &h.ro,
            &format!("SELECT count(*) FROM bodies WHERE sanitizer_version < {current}"),
        ) == 0
    })
    .await;
    let html: String = h
        .ro
        .query_row(
            "SELECT html_sanitized FROM bodies b JOIN messages m ON m.id = b.message_id WHERE m.gmail_id = 'm1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!html.contains("<p>old</p>") && !html.contains("script"), "{html}");
    assert_eq!(fts_hits(&h.ro, "pineapple1"), 1);
}

#[tokio::test(start_paused = true)]
async fn backfill_stores_messages_bodies_and_fts() {
    let h = backfilled().await;
    let c = &h.ro;
    assert_eq!(count(c, "SELECT count(*) FROM messages"), 130);
    assert_eq!(count(c, "SELECT count(*) FROM threads"), 44);
    assert_eq!(count(c, "SELECT count(*) FROM bodies"), 130);
    assert!(h.gmail.st().max_batch <= 50);
    // newest message first in the inbox
    let page = tidemail_db::read::list_threads(
        c,
        &tidemail_db::types::View {
            label_key: "INBOX".into(),
            account_id: None,
        },
        None,
        100,
    )
    .unwrap();
    assert_eq!(page.items.len(), 44);
    assert!(page.items[0].last_date >= page.items[1].last_date);
    // sanitized, text searchable, metadata searchable
    let html: String = c
        .query_row(
            "SELECT html_sanitized FROM bodies b JOIN messages m ON m.id = b.message_id WHERE m.gmail_id = 'm1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!html.contains("script") && !html.contains("evil"));
    assert_eq!(fts_hits(c, "pineapple17"), 1);
    assert_eq!(fts_hits(c, "subject:\"Subject 17\""), 1);
    assert_eq!(fts_hits(c, "from_email:s17*"), 1);
    c.execute(
        "INSERT INTO messages_fts(messages_fts) VALUES('integrity-check')",
        [],
    )
    .unwrap();
    // inline blobs: m0,m10,... inline data; m4 fetched via get_attachment
    assert_eq!(count(c, "SELECT count(*) FROM inline_blobs"), 14);
    assert_eq!(
        count(
            c,
            "SELECT has_attachment FROM messages WHERE gmail_id = 'm3'"
        ),
        1
    );
    assert_eq!(
        count(
            c,
            "SELECT has_attachment FROM messages WHERE gmail_id = 'm4'"
        ),
        0
    );
    // flags and labels
    assert_eq!(
        count(c, "SELECT is_unread FROM messages WHERE gmail_id = 'm5'"),
        1
    );
    assert_eq!(
        count(c, "SELECT is_starred FROM messages WHERE gmail_id = 'm7'"),
        1
    );
    assert!(h.engine.status().accounts[0].state == AccountSyncState::Idle);
    assert!(h.engine.status().accounts[0].last_sync_at.is_some());
}

#[tokio::test(start_paused = true)]
async fn backfill_resumes_after_offline() {
    let g = FakeGmail::new();
    big_mailbox(&g);
    g.st().offline_after_gets = Some(2);
    let h = setup(cfg(), true, g);
    until(|| h.engine.status().accounts[0].state == AccountSyncState::Offline).await;
    let partial = count(&h.ro, "SELECT count(*) FROM messages");
    assert!(partial > 0 && partial < 130, "partial = {partial}");
    let cursor: Option<String> =
        h.ro.query_row("SELECT backfill_cursor FROM sync_state", [], |r| r.get(0))
            .unwrap();
    assert_ne!(cursor.as_deref(), Some("done"));
    h.gmail.st().offline = false;
    until(|| backfill_done(&h.ro)).await;
    assert_eq!(count(&h.ro, "SELECT count(*) FROM messages"), 130);
    assert_eq!(count(&h.ro, "SELECT count(*) FROM bodies"), 130);
}

#[tokio::test(start_paused = true)]
async fn big_accounts_defer_old_bodies_until_opened() {
    let g = FakeGmail::new();
    let base = tidemail_db::now_ms();
    let year = 365 * 24 * 60;
    for i in 0..30 {
        let age = if i < 10 { i } else { 3 * year + i };
        g.add(msg(i, i, &["INBOX"], age, base));
    }
    let h = setup(
        Config {
            body_policy_threshold: 20,
            ..cfg()
        },
        true,
        g,
    );
    until(|| count(&h.ro, "SELECT count(*) FROM messages WHERE body_state = 1") == 10).await;
    secs(30).await;
    assert_eq!(
        count(&h.ro, "SELECT count(*) FROM messages WHERE body_state = 2"),
        20
    );
    assert_eq!(
        count(&h.ro, "SELECT count(*) FROM messages WHERE body_state = 1"),
        10
    );
    // opening an old message prefetches its body
    let old: i64 =
        h.ro.query_row(
            "SELECT id FROM messages WHERE body_state = 2 LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    h.engine.prefetch_bodies(&[old]);
    until(|| {
        count(
            &h.ro,
            &format!("SELECT body_state FROM messages WHERE id = {old}"),
        ) == 1
    })
    .await;
    assert_eq!(
        count(&h.ro, "SELECT count(*) FROM messages WHERE body_state = 2"),
        19
    );
}

// ---- live changes ----

#[tokio::test(start_paused = true)]
async fn history_applies_add_label_change_and_delete() {
    let h = backfilled().await;
    let base = tidemail_db::now_ms();
    h.gmail.add(msg(500, 900, &["INBOX", "UNREAD"], 0, base));
    h.gmail.modify_thread("t1", &[], &["UNREAD", "INBOX"]);
    h.gmail.delete("m129");
    h.rec.data.lock().unwrap().clear();
    until(|| {
        count(
            &h.ro,
            "SELECT count(*) FROM messages WHERE gmail_id = 'm500'",
        ) == 1
    })
    .await;
    until(|| {
        count(
            &h.ro,
            "SELECT count(*) FROM messages WHERE gmail_id = 'm129'",
        ) == 0
    })
    .await;
    until(|| backfill_done(&h.ro)).await;

    let t1 = thread_id(&h.ro, "t1");
    assert!(!in_label(&h.ro, t1, "INBOX"));
    assert_eq!(
        count(
            &h.ro,
            &format!("SELECT unread_count FROM threads WHERE id = {t1}")
        ),
        0
    );
    let t900 = thread_id(&h.ro, "t900");
    assert!(in_label(&h.ro, t900, "INBOX"));
    assert_eq!(fts_hits(&h.ro, "pineapple129"), 0);
    assert_eq!(fts_hits(&h.ro, "pineapple500"), 1);
    // the UI heard about the inbox
    assert!(h
        .rec
        .data
        .lock()
        .unwrap()
        .iter()
        .any(|(t, k)| t.contains(&t1) && k.iter().any(|k| k == "INBOX")));
    h.ro.execute(
        "INSERT INTO messages_fts(messages_fts) VALUES('integrity-check')",
        [],
    )
    .unwrap();
}

#[tokio::test(start_paused = true)]
async fn expired_history_runs_full_id_diff() {
    let h = backfilled().await;
    let base = tidemail_db::now_ms();
    h.gmail.add(msg(600, 901, &["INBOX"], 0, base));
    h.gmail.delete("m100");
    h.gmail.modify_thread("t2", &["STARRED"], &["INBOX"]);
    h.gmail.expire_history();
    until(|| {
        count(
            &h.ro,
            "SELECT count(*) FROM messages WHERE gmail_id = 'm600'",
        ) == 1
    })
    .await;
    until(|| {
        count(
            &h.ro,
            "SELECT count(*) FROM messages WHERE gmail_id = 'm100'",
        ) == 0
    })
    .await;
    until(|| backfill_done(&h.ro)).await;
    let t2 = thread_id(&h.ro, "t2");
    assert!(!in_label(&h.ro, t2, "INBOX"));
    assert!(in_label(&h.ro, t2, "STARRED"));
    // history id moved on: normal polling resumes
    h.gmail.modify_thread("t3", &[], &["INBOX"]);
    let t3 = thread_id(&h.ro, "t3");
    until(|| !in_label(&h.ro, t3, "INBOX")).await;
}

// ---- action queue ----

#[tokio::test(start_paused = true)]
async fn action_applies_locally_then_sends_after_undo_window() {
    let h = backfilled().await;
    let t = thread_id(&h.ro, "t1"); // m3,m4,m5: m5 unread
    let ticket = h
        .engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::Archive,
        })
        .unwrap();
    assert_eq!(ticket.action_ids.len(), 1);
    assert!(!in_label(&h.ro, t, "INBOX"));
    assert_eq!(h.engine.status().pending_actions, 1);
    let due: i64 =
        h.ro.query_row(
            "SELECT next_attempt_at - created_at FROM action_queue",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(due, 5000);
    assert!(h
        .rec
        .data
        .lock()
        .unwrap()
        .iter()
        .any(|(ts, _)| ts == &vec![t]));
    secs(4).await;
    assert!(h.gmail.st().modify_calls.is_empty());
    secs(2).await;
    let calls = h.gmail.st().modify_calls.clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        (&calls[0].1[..], &calls[0].2, &calls[0].3),
        ("t1", &vec![], &vec!["INBOX".to_owned()])
    );
    assert_eq!(h.engine.status().pending_actions, 0);
}

#[tokio::test(start_paused = true)]
async fn undo_inside_window_sends_nothing_and_restores_exactly() {
    let h = backfilled().await;
    let t = thread_id(&h.ro, "t1");
    let unread_before = count(
        &h.ro,
        &format!("SELECT unread_count FROM threads WHERE id = {t}"),
    );
    assert_eq!(unread_before, 1);
    let ticket = h
        .engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::MarkRead,
        })
        .unwrap();
    assert_eq!(
        count(
            &h.ro,
            &format!("SELECT unread_count FROM threads WHERE id = {t}")
        ),
        0
    );
    secs(2).await;
    h.engine.undo_action(&ticket.action_ids).unwrap();
    // only m5 was unread: exact restore, not "mark all unread"
    assert_eq!(
        count(
            &h.ro,
            &format!("SELECT unread_count FROM threads WHERE id = {t}")
        ),
        1
    );
    assert_eq!(count(&h.ro, "SELECT count(*) FROM action_queue"), 0);
    secs(30).await;
    assert!(h.gmail.st().modify_calls.is_empty());
}

#[tokio::test(start_paused = true)]
async fn undo_after_window_sends_inverse() {
    let h = backfilled().await;
    let t = thread_id(&h.ro, "t1");
    let ticket = h
        .engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::Archive,
        })
        .unwrap();
    secs(6).await;
    assert_eq!(h.gmail.st().modify_calls.len(), 1);
    h.engine.undo_action(&ticket.action_ids).unwrap();
    assert!(in_label(&h.ro, t, "INBOX"));
    secs(1).await;
    let calls = h.gmail.st().modify_calls.clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        (&calls[1].2, &calls[1].3),
        (&vec!["INBOX".to_owned()], &vec![])
    );
    assert!(h.gmail.labels_of("t1").contains("INBOX"));
}

#[tokio::test(start_paused = true)]
async fn move_is_one_modify_and_trash_uses_trash_endpoint() {
    let h = backfilled().await;
    let (t1, t2) = (thread_id(&h.ro, "t1"), thread_id(&h.ro, "t2"));
    let key = "u:1:Label_1".to_owned();
    h.engine
        .apply_action(ActionRequest {
            thread_ids: vec![t1],
            kind: ActionKind::Move {
                label_key: key.clone(),
            },
        })
        .unwrap();
    h.engine
        .apply_action(ActionRequest {
            thread_ids: vec![t2],
            kind: ActionKind::Trash,
        })
        .unwrap();
    assert!(in_label(&h.ro, t1, &key) && !in_label(&h.ro, t1, "INBOX"));
    assert!(in_label(&h.ro, t2, "TRASH") && !in_label(&h.ro, t2, "INBOX"));
    secs(7).await;
    let s = h.gmail.st();
    assert_eq!(s.modify_calls.len(), 1);
    assert_eq!(s.modify_calls[0].2, vec!["Label_1".to_owned()]);
    assert_eq!(s.modify_calls[0].3, vec!["INBOX".to_owned()]);
    assert_eq!(s.other_calls, vec!["trash t2".to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn failing_action_backs_off_then_fails_and_gmail_state_returns() {
    let h = backfilled().await;
    let t = thread_id(&h.ro, "t0"); // m0 starred+unread, m1, m2
    h.gmail.st().fail_modify = Some(ApiError::Other("boom".into()));
    let ticket = h
        .engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::Archive,
        })
        .unwrap();
    assert!(!in_label(&h.ro, t, "INBOX"));
    until_long(|| !h.engine.status().failed_actions.is_empty()).await;

    let calls = h.gmail.st().modify_calls.clone();
    assert_eq!(calls.len(), 8);
    let gaps: Vec<u64> = calls
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).as_secs())
        .collect();
    assert!(
        gaps.windows(2).all(|g| g[1] >= g[0]),
        "gaps must not shrink: {gaps:?}"
    );
    assert!(gaps[2] >= 2 * gaps[0], "exponential: {gaps:?}");

    let st = h.engine.status();
    assert_eq!(st.pending_actions, 0);
    assert_eq!(st.failed_actions.len(), 1);
    assert_eq!(st.failed_actions[0].id, ticket.action_ids[0]);
    assert_eq!(st.failed_actions[0].error, "boom");
    // next sync restores Gmail's state (still in the inbox)
    until(|| in_label(&h.ro, t, "INBOX")).await;

    // Retry: re-applied locally and sent
    h.gmail.st().fail_modify = None;
    h.engine.retry_failed(ticket.action_ids[0]).unwrap();
    assert!(!in_label(&h.ro, t, "INBOX"));
    secs(1).await;
    assert_eq!(h.gmail.st().modify_calls.len(), 9);
    assert!(h.engine.status().failed_actions.is_empty());
    assert!(!h.gmail.labels_of("t0").contains("INBOX"));
}

#[tokio::test(start_paused = true)]
async fn dismiss_failed_restores_gmail_state() {
    let h = backfilled().await;
    let t = thread_id(&h.ro, "t0");
    h.gmail.st().fail_modify = Some(ApiError::Other("nope".into()));
    h.engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::Star,
        })
        .unwrap();
    until_long(|| !h.engine.status().failed_actions.is_empty()).await;
    let id = h.engine.status().failed_actions[0].id;
    h.engine.dismiss_failed(id).unwrap();
    assert!(h.engine.status().failed_actions.is_empty());
    assert_eq!(count(&h.ro, "SELECT count(*) FROM action_queue"), 0);
}

#[tokio::test(start_paused = true)]
async fn offline_actions_wait_without_using_attempts() {
    let h = backfilled().await;
    let t = thread_id(&h.ro, "t1");
    h.gmail.st().offline = true;
    h.engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::Star,
        })
        .unwrap();
    secs(120).await;
    let attempts: i64 =
        h.ro.query_row("SELECT attempts FROM action_queue", [], |r| r.get(0))
            .unwrap();
    assert_eq!(attempts, 0);
    assert_eq!(h.engine.status().pending_actions, 1);
    h.gmail.st().offline = false;
    secs(70).await;
    assert_eq!(h.engine.status().pending_actions, 0);
    assert!(h.gmail.labels_of("t1").contains("STARRED"));
}

#[tokio::test(start_paused = true)]
async fn remote_change_keeps_pending_action_on_top() {
    let g = FakeGmail::new();
    big_mailbox(&g);
    let h = setup(
        Config {
            undo_delay: Duration::from_secs(600),
            ..cfg()
        },
        true,
        g,
    );
    until(|| backfill_done(&h.ro)).await;
    let t = thread_id(&h.ro, "t2");
    h.engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::Archive,
        })
        .unwrap();
    // elsewhere: star the thread and a reply arrives (in the inbox)
    h.gmail.modify_thread("t2", &["STARRED"], &[]);
    h.gmail
        .add(msg(700, 2, &["INBOX", "UNREAD"], 0, tidemail_db::now_ms()));
    until(|| {
        count(
            &h.ro,
            "SELECT count(*) FROM messages WHERE gmail_id = 'm700'",
        ) == 1
    })
    .await;
    until(|| in_label(&h.ro, t, "STARRED")).await;
    // remote change applied, pending archive applied on top (also to the reply)
    assert!(!in_label(&h.ro, t, "INBOX"));
    assert_eq!(
        count(
            &h.ro,
            &format!("SELECT message_count FROM threads WHERE id = {t}")
        ),
        4
    );
    assert!(h.gmail.st().modify_calls.is_empty());
}

#[tokio::test(start_paused = true)]
async fn action_write_works_without_a_provider() {
    let dir = std::env::temp_dir().join(format!("tidemail-sync-test-np-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mail.db");
    let t;
    {
        let conn = tidemail_db::open(&path).unwrap();
        let a = write::insert_account(
            &conn,
            &NewAccount {
                email: "a@x.com",
                display_name: None,
                color: "#000",
                sort_order: 0,
            },
        )
        .unwrap();
        let inbox = write::upsert_label(&conn, a, "INBOX", "Inbox", true, None).unwrap();
        t = write::ensure_thread(&conn, a, "t").unwrap();
        let m = write::insert_message(
            &conn,
            &write::NewMessage {
                account_id: a,
                gmail_id: "m",
                thread_id: t,
                from_name: "A",
                from_email: "a@x.com",
                to_text: "",
                subject: "s",
                snippet: "",
                internal_date: 1,
                is_unread: true,
                is_starred: false,
                has_attachment: false,
                body_state: tidemail_db::types::BodyState::Missing,
            },
        )
        .unwrap();
        let unread = write::upsert_label(&conn, a, "UNREAD", "Unread", true, None).unwrap();
        write::set_message_labels(&conn, m, &[inbox, unread]).unwrap();
        write::recompute_thread(&conn, t).unwrap();
    }
    let engine = Engine::start_on(
        Handle::current(),
        path.clone(),
        Arc::new(Rec::default()),
        None,
        cfg(),
    );
    let tk = engine
        .apply_action(ActionRequest {
            thread_ids: vec![t],
            kind: ActionKind::MarkRead,
        })
        .unwrap();
    let c = tidemail_db::open(&path).unwrap();
    assert_eq!(count(&c, "SELECT unread_count FROM threads"), 0);
    engine.undo_action(&tk.action_ids).unwrap();
    assert_eq!(count(&c, "SELECT unread_count FROM threads"), 1);
    drop(c);
    let _ = std::fs::remove_dir_all(dir);
}

// ---- accounts, attachments ----

#[tokio::test(start_paused = true)]
async fn add_and_remove_account() {
    let g = FakeGmail::new();
    big_mailbox(&g);
    let h = setup(cfg(), false, g);
    assert!(h.engine.status().accounts.is_empty());
    let a = h.engine.add_account().await.unwrap();
    assert_eq!(a.email, "me@example.com");
    until(|| backfill_done(&h.ro)).await;
    assert_eq!(count(&h.ro, "SELECT count(*) FROM messages"), 130);
    assert_eq!(h.engine.status().accounts.len(), 1);

    h.engine.remove_account(a.id).await.unwrap();
    assert_eq!(count(&h.ro, "SELECT count(*) FROM accounts"), 0);
    assert_eq!(count(&h.ro, "SELECT count(*) FROM messages"), 0);
    assert_eq!(count(&h.ro, "SELECT count(*) FROM thread_labels"), 0);
    assert_eq!(fts_hits(&h.ro, "pineapple17"), 0);
    h.ro.execute(
        "INSERT INTO messages_fts(messages_fts) VALUES('integrity-check')",
        [],
    )
    .unwrap();
}

#[tokio::test(start_paused = true)]
async fn download_attachment_saves_file_once() {
    let h = backfilled().await;
    let id: i64 =
        h.ro.query_row(
            "SELECT id FROM attachments WHERE filename = 'report.pdf'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let p = h.engine.download_attachment(id).await.unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"bytes-of-att3");
    assert!(p.starts_with(&h.dir));
    let fetches = h.gmail.st().attachment_fetches;
    let again = h.engine.download_attachment(id).await.unwrap();
    assert_eq!(again, p);
    assert_eq!(h.gmail.st().attachment_fetches, fetches);
    let stored: String =
        h.ro.query_row(
            "SELECT local_path FROM attachments WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(PathBuf::from(stored), p);
    let _ = &h.path;
}

// ---- sync window ----

const DAY_MIN: i64 = 24 * 60;

fn window_cfg() -> Config {
    Config {
        sync_window: Some(Duration::from_secs(183 * 24 * 3600)),
        ..cfg()
    }
}

fn account_id(c: &Connection) -> i64 {
    count(c, "SELECT id FROM accounts")
}

#[tokio::test(start_paused = true)]
async fn new_account_lists_only_the_window_and_settings_extend_it() {
    let g = FakeGmail::new();
    let base = tidemail_db::now_ms();
    for i in 0..30 {
        let age = if i < 10 { i * DAY_MIN } else { 400 * DAY_MIN + i };
        g.add(msg(i, i, &["INBOX"], age, base));
    }
    let h = setup(window_cfg(), true, g);
    until(|| backfill_done(&h.ro)).await;
    secs(30).await;
    assert_eq!(count(&h.ro, "SELECT count(*) FROM messages"), 10);
    let a = h.engine.status().accounts[0].clone();
    assert!(a.sync_from.is_some_and(|f| f < base - 182 * DAY_MIN * 60_000));
    assert_eq!(a.backfill, None);

    // A narrower window is refused; a wider one starts the backfill again.
    let id = account_id(&h.ro);
    h.engine.extend_sync(id, Some(base)).unwrap();
    assert_eq!(h.engine.status().accounts[0].sync_from, a.sync_from);
    h.engine.extend_sync(id, None).unwrap();
    let b = h.engine.status().accounts[0].clone();
    assert_eq!(b.sync_from, None);
    assert!(b.backfill.is_some_and(|b| b.older));
    until(|| count(&h.ro, "SELECT count(*) FROM messages") == 30).await;
    until(|| backfill_done(&h.ro)).await;
    assert_eq!(h.engine.status().accounts[0].backfill, None);
}

#[tokio::test(start_paused = true)]
async fn first_sync_progress_follows_the_window() {
    let g = FakeGmail::new();
    let base = tidemail_db::now_ms();
    for i in 0..10 {
        g.add(msg(i, i, &["INBOX"], i * DAY_MIN, base));
    }
    g.st().offline = true;
    let h = setup(window_cfg(), true, g);
    // Offline before the first page: the window is set, nothing stored.
    until(|| count(&h.ro, "SELECT count(*) FROM sync_state") == 1).await;
    let s = h.engine.status().accounts[0].clone();
    assert_eq!(s.backfill.map(|b| (b.percent, b.older)), Some((0, false)));
}

#[tokio::test(start_paused = true)]
async fn full_diff_keeps_stored_mail_older_than_the_window() {
    let g = FakeGmail::new();
    let base = tidemail_db::now_ms();
    for i in 0..10 {
        g.add(msg(i, i, &["INBOX"], i * DAY_MIN, base));
    }
    let h = setup(window_cfg(), true, g);
    until(|| backfill_done(&h.ro)).await;
    // Mail of days 6..9 is stored but now outside the window.
    h.ro.execute(
        "UPDATE sync_state SET sync_from = ?1",
        [base - 5 * DAY_MIN * 60_000],
    )
    .unwrap();
    h.gmail.delete("m1");
    h.gmail.expire_history();
    until(|| count(&h.ro, "SELECT count(*) FROM messages WHERE gmail_id = 'm1'") == 0).await;
    assert_eq!(count(&h.ro, "SELECT count(*) FROM messages"), 9);
}
