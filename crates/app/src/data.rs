//! Database access for the UI. Reads run on GPUI's background executor with
//! pooled read connections; the UI thread never touches SQLite.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui_kit::{App, AppContext, Task};
use tidemail_db::{
    read,
    types::{Account, Label, ThreadPage, UnreadCount, View},
    Connection,
};

/// Rows per list page.
pub const PAGE: u32 = 100;

pub type DbResult<T> = Result<T, String>;

pub struct Db {
    path: PathBuf,
    idle: Mutex<Vec<Connection>>,
}

impl Db {
    pub fn new(path: PathBuf, first: Connection) -> Arc<Self> {
        Arc::new(Self {
            path,
            idle: Mutex::new(vec![first]),
        })
    }

    /// Run `f` on a pooled connection on the calling thread (background threads only).
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> tidemail_db::Result<T>) -> DbResult<T> {
        let conn = self.idle.lock().expect("db pool").pop();
        let conn = match conn {
            Some(c) => c,
            None => tidemail_db::open(&self.path).map_err(|e| e.to_string())?,
        };
        let out = f(&conn).map_err(|e| e.to_string());
        self.idle.lock().expect("db pool").push(conn);
        out
    }

    /// Run `f` on the background executor.
    pub fn read<T: Send + 'static>(
        self: &Arc<Self>,
        cx: &App,
        f: impl FnOnce(&Connection) -> tidemail_db::Result<T> + Send + 'static,
    ) -> Task<DbResult<T>> {
        let db = self.clone();
        cx.background_spawn(async move { db.with(f) })
    }
}

/// What the first frame needs. Loaded on a thread that runs while GPUI starts.
pub struct Preload {
    pub db: Arc<Db>,
    pub accounts: Vec<Account>,
    pub labels: Vec<Label>,
    pub unread: Vec<UnreadCount>,
    pub first_page: ThreadPage,
}

pub fn inbox() -> View {
    View {
        label_key: "INBOX".into(),
        account_id: None,
    }
}

pub fn preload(path: PathBuf) -> DbResult<Preload> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = tidemail_db::open(&path).map_err(|e| e.to_string())?;
    let load = || -> tidemail_db::Result<_> {
        Ok((
            read::list_accounts(&conn)?,
            read::list_labels(&conn, None)?,
            read::unread_counts(&conn)?,
            read::list_threads(&conn, &inbox(), None, PAGE)?,
        ))
    };
    let (accounts, labels, unread, first_page) = load().map_err(|e| e.to_string())?;
    Ok(Preload {
        db: Db::new(path, conn),
        accounts,
        labels,
        unread,
        first_page,
    })
}
