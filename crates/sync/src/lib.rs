//! Sync engine: backfill, `history.list` polling, full id diff, action queue
//! (undo delay, reconciliation, failure), body prefetch.
//!
//! Phase 0 state: stub. Phase 1 (sync agent) owns this crate. Keep the
//! `Engine` method signatures and the `Notifier` trait; the app crate calls
//! them from Tauri commands.

use std::path::PathBuf;
use std::sync::Arc;

use tidemail_db::types::{Account, ActionRequest, ActionTicket, SyncStatus};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet")]
    NotImplemented,
    #[error(transparent)]
    Db(#[from] tidemail_db::Error),
    #[error(transparent)]
    Gmail(#[from] tidemail_gmail::Error),
}

/// Push channel to the UI. The app crate implements it with Tauri events.
pub trait Notifier: Send + Sync + 'static {
    /// Rows changed; the UI re-queries what is on screen.
    fn data_changed(&self, thread_ids: Vec<i64>, label_keys: Vec<String>);
    fn status_changed(&self, status: SyncStatus);
}

pub struct Engine {
    #[allow(dead_code)]
    db_path: PathBuf,
    #[allow(dead_code)]
    notifier: Arc<dyn Notifier>,
}

impl Engine {
    /// Open own DB connections and start background tasks (on the Tauri
    /// async runtime). Must return at once: it is on the cold-launch path.
    pub fn start(db_path: PathBuf, notifier: Arc<dyn Notifier>) -> Self {
        Self { db_path, notifier }
    }

    /// Apply locally at once, enqueue for Gmail with a 5 s undo delay.
    pub fn apply_action(&self, req: ActionRequest) -> Result<ActionTicket, Error> {
        let _ = req;
        Err(Error::NotImplemented)
    }

    /// Inside 5 s: delete the queue rows. Later: enqueue the inverse.
    pub fn undo_action(&self, action_ids: &[i64]) -> Result<(), Error> {
        let _ = action_ids;
        Err(Error::NotImplemented)
    }

    pub fn retry_failed(&self, action_id: i64) -> Result<(), Error> {
        let _ = action_id;
        Err(Error::NotImplemented)
    }

    pub fn dismiss_failed(&self, action_id: i64) -> Result<(), Error> {
        let _ = action_id;
        Err(Error::NotImplemented)
    }

    /// Fetch bodies for these messages soon (next 10 in the list).
    pub fn prefetch_bodies(&self, message_ids: &[i64]) {
        let _ = message_ids;
    }

    pub fn status(&self) -> SyncStatus {
        SyncStatus::default()
    }

    /// Browser sign-in, insert the account, start its backfill.
    pub async fn add_account(&self) -> Result<Account, Error> {
        Err(Error::NotImplemented)
    }

    pub async fn remove_account(&self, account_id: i64) -> Result<(), Error> {
        let _ = account_id;
        Err(Error::NotImplemented)
    }

    /// Download to the attachment cache (with Mark-of-the-Web); returns the path.
    pub async fn download_attachment(&self, attachment_id: i64) -> Result<PathBuf, Error> {
        let _ = attachment_id;
        Err(Error::NotImplemented)
    }
}
