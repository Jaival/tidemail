use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DataChanged {
    pub thread_ids: Vec<i64>,
    pub label_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusChanged {
    pub status: tidemail_db::types::SyncStatus,
}
