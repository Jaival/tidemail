//! Events from background threads (sync engine, tray, second launch) to the UI thread.

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use tidemail_db::types::SyncStatus;

#[derive(Debug)]
pub enum AppEvent {
    DataChanged {
        thread_ids: Vec<i64>,
        label_keys: Vec<String>,
    },
    StatusChanged(SyncStatus),
    ShowWindow,
    Quit,
}

#[derive(Clone)]
pub struct EventTx(UnboundedSender<AppEvent>);

impl EventTx {
    pub fn send(&self, event: AppEvent) {
        // Fails only after the UI has shut down.
        let _ = self.0.unbounded_send(event);
    }
}

pub fn channel() -> (EventTx, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = unbounded();
    (EventTx(tx), rx)
}

impl tidemail_sync::Notifier for EventTx {
    fn data_changed(&self, thread_ids: Vec<i64>, label_keys: Vec<String>) {
        self.send(AppEvent::DataChanged {
            thread_ids,
            label_keys,
        });
    }
    fn status_changed(&self, status: SyncStatus) {
        self.send(AppEvent::StatusChanged(status));
    }
}
