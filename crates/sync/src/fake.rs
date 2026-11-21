//! In-memory fake Gmail for tests.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use tokio::time::Instant;

use crate::api::*;

#[derive(Clone)]
pub struct FakeMsg {
    pub id: String,
    pub thread_id: String,
    pub labels: BTreeSet<String>,
    pub date: i64,
    pub from: String,
    pub subject: String,
    pub html: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<RemoteAttachment>,
}

pub enum Event {
    Added(String),
    Deleted(String),
    Labels(String),
}

pub struct FakeState {
    pub msgs: Vec<FakeMsg>,
    pub history: Vec<(u64, Event)>,
    pub hid: u64,
    pub floor: u64,
    pub page_size: usize,
    pub offline: bool,
    /// Go offline after this many more `get_messages` calls.
    pub offline_after_gets: Option<usize>,
    pub fail_modify: Option<ApiError>,
    pub modify_calls: Vec<(Instant, String, Vec<String>, Vec<String>)>,
    pub other_calls: Vec<String>,
    pub max_batch: usize,
    pub attachment_fetches: usize,
}

pub struct FakeGmail(pub Mutex<FakeState>);

pub fn label_set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

impl FakeGmail {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(FakeState {
            msgs: vec![],
            history: vec![],
            hid: 100,
            floor: 0,
            page_size: 40,
            offline: false,
            offline_after_gets: None,
            fail_modify: None,
            modify_calls: vec![],
            other_calls: vec![],
            max_batch: 0,
            attachment_fetches: 0,
        })))
    }

    pub fn st(&self) -> MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }

    pub fn add(&self, m: FakeMsg) {
        let mut s = self.st();
        s.hid += 1;
        let hid = s.hid;
        s.history.push((hid, Event::Added(m.id.clone())));
        s.msgs.push(m);
    }

    /// Remote-side thread label change (as if done in another client).
    pub fn modify_thread(&self, thread: &str, add: &[&str], remove: &[&str]) {
        let mut s = self.st();
        s.hid += 1;
        let hid = s.hid;
        let mut ids = vec![];
        for m in s.msgs.iter_mut().filter(|m| m.thread_id == thread) {
            for a in add {
                m.labels.insert((*a).to_owned());
            }
            for r in remove {
                m.labels.remove(*r);
            }
            ids.push(m.id.clone());
        }
        for id in ids {
            s.history.push((hid, Event::Labels(id)));
        }
    }

    pub fn delete(&self, id: &str) {
        let mut s = self.st();
        s.hid += 1;
        let hid = s.hid;
        s.msgs.retain(|m| m.id != id);
        s.history.push((hid, Event::Deleted(id.to_owned())));
    }

    pub fn expire_history(&self) {
        let mut s = self.st();
        s.floor = s.hid + 1;
    }

    pub fn labels_of(&self, thread: &str) -> BTreeSet<String> {
        self.st()
            .msgs
            .iter()
            .filter(|m| m.thread_id == thread)
            .flat_map(|m| m.labels.iter().cloned())
            .collect()
    }

    fn check_online(s: &FakeState) -> ApiResult<()> {
        if s.offline {
            Err(ApiError::Offline("fake offline".into()))
        } else {
            Ok(())
        }
    }
}

fn remote(m: &FakeMsg, format: Format) -> RemoteMessage {
    RemoteMessage {
        id: m.id.clone(),
        thread_id: m.thread_id.clone(),
        label_ids: m.labels.iter().cloned().collect(),
        snippet: m
            .text
            .clone()
            .unwrap_or_default()
            .chars()
            .take(40)
            .collect(),
        internal_date_ms: m.date,
        from: m.from.clone(),
        to: "me@example.com".into(),
        cc: String::new(),
        subject: m.subject.clone(),
        has_attachment: false,
        body: (format == Format::Full).then(|| RemoteBody {
            html: m.html.clone(),
            text: m.text.clone(),
            attachments: m.attachments.clone(),
        }),
    }
}

#[async_trait]
impl GmailApi for FakeGmail {
    async fn get_profile(&self) -> ApiResult<Profile> {
        let s = self.st();
        Self::check_online(&s)?;
        Ok(Profile {
            email: "me@example.com".into(),
            history_id: s.hid.to_string(),
            messages_total: Some(s.msgs.len() as i64),
        })
    }

    async fn list_message_ids(
        &self,
        label: Option<&str>,
        query: Option<&str>,
        page_token: Option<&str>,
    ) -> ApiResult<IdPage> {
        let s = self.st();
        Self::check_online(&s)?;
        // Only the query the sync sends: `after:<epoch seconds>`.
        let after_ms = query.map(|q| {
            let secs: i64 = q.strip_prefix("after:").and_then(|n| n.parse().ok()).expect("after:<secs>");
            secs * 1000
        });
        let mut v: Vec<&FakeMsg> = s
            .msgs
            .iter()
            .filter(|m| label.is_none_or(|l| m.labels.contains(l)))
            .filter(|m| after_ms.is_none_or(|a| m.date > a))
            .collect();
        v.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        let start: usize = page_token.map_or(0, |t| t.parse().unwrap());
        let end = (start + s.page_size).min(v.len());
        Ok(IdPage {
            ids: v[start..end]
                .iter()
                .map(|m| MessageRef {
                    id: m.id.clone(),
                    thread_id: m.thread_id.clone(),
                })
                .collect(),
            next_page_token: (end < v.len()).then(|| end.to_string()),
        })
    }

    async fn get_messages(&self, ids: &[String], format: Format) -> ApiResult<Vec<RemoteMessage>> {
        let mut s = self.st();
        Self::check_online(&s)?;
        assert!(ids.len() <= 50, "batch too large: {}", ids.len());
        s.max_batch = s.max_batch.max(ids.len());
        if let Some(n) = s.offline_after_gets {
            if n == 0 {
                s.offline = true;
                s.offline_after_gets = None;
                return Err(ApiError::Offline("dropped".into()));
            }
            s.offline_after_gets = Some(n - 1);
        }
        Ok(ids
            .iter()
            .filter_map(|id| s.msgs.iter().find(|m| &m.id == id))
            .map(|m| remote(m, format))
            .collect())
    }

    async fn get_attachment(&self, _message_id: &str, attachment_id: &str) -> ApiResult<Vec<u8>> {
        let mut s = self.st();
        Self::check_online(&s)?;
        s.attachment_fetches += 1;
        Ok(format!("bytes-of-{attachment_id}").into_bytes())
    }

    async fn list_history(&self, start: &str, _page_token: Option<&str>) -> ApiResult<HistoryPage> {
        let s = self.st();
        Self::check_online(&s)?;
        let start: u64 = start.parse().unwrap();
        if start < s.floor {
            return Err(ApiError::HistoryExpired);
        }
        let mut p = HistoryPage {
            history_id: s.hid.to_string(),
            ..Default::default()
        };
        for (h, e) in s.history.iter().filter(|(h, _)| *h > start) {
            let _ = h;
            match e {
                Event::Added(id) => p.added.push(id.clone()),
                Event::Deleted(id) => p.deleted.push(id.clone()),
                Event::Labels(id) => p.label_changed.push(id.clone()),
            }
        }
        Ok(p)
    }

    async fn list_labels(&self) -> ApiResult<Vec<RemoteLabel>> {
        let s = self.st();
        Self::check_online(&s)?;
        let sys = |id: &str| RemoteLabel {
            id: id.into(),
            name: id.into(),
            is_system: true,
            color: None,
        };
        Ok(vec![
            sys("INBOX"),
            sys("SENT"),
            sys("UNREAD"),
            sys("STARRED"),
            sys("TRASH"),
            sys("SPAM"),
            RemoteLabel {
                id: "Label_1".into(),
                name: "Work".into(),
                is_system: false,
                color: None,
            },
        ])
    }

    async fn threads_modify(
        &self,
        thread_id: &str,
        add: &[String],
        remove: &[String],
    ) -> ApiResult<()> {
        {
            let mut s = self.st();
            Self::check_online(&s)?;
            s.modify_calls.push((
                Instant::now(),
                thread_id.to_owned(),
                add.to_vec(),
                remove.to_vec(),
            ));
            if let Some(e) = s.fail_modify.clone() {
                return Err(e);
            }
        }
        let a: Vec<&str> = add.iter().map(String::as_str).collect();
        let r: Vec<&str> = remove.iter().map(String::as_str).collect();
        self.modify_thread(thread_id, &a, &r);
        Ok(())
    }

    async fn threads_trash(&self, thread_id: &str) -> ApiResult<()> {
        Self::check_online(&self.st())?;
        self.st().other_calls.push(format!("trash {thread_id}"));
        self.modify_thread(thread_id, &["TRASH"], &["INBOX"]);
        Ok(())
    }

    async fn threads_untrash(&self, thread_id: &str) -> ApiResult<()> {
        Self::check_online(&self.st())?;
        self.st().other_calls.push(format!("untrash {thread_id}"));
        self.modify_thread(thread_id, &[], &["TRASH"]);
        Ok(())
    }
}

pub struct FakeProvider {
    pub api: Arc<FakeGmail>,
    pub email: String,
}

#[async_trait]
impl GmailProvider for FakeProvider {
    async fn sign_in(&self) -> ApiResult<SignedInAccount> {
        Ok(SignedInAccount {
            email: self.email.clone(),
            display_name: Some("Fake Me".into()),
        })
    }
    fn api(&self, _email: &str) -> ApiResult<Arc<dyn GmailApi>> {
        Ok(self.api.clone())
    }
    fn sign_out(&self, _email: &str) -> ApiResult<()> {
        Ok(())
    }
}
