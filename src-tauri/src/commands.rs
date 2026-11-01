use specta::specta;
use tauri::State;
use tidemail_db::{read, types::*};

use crate::AppState;

#[tauri::command(async)]
#[specta]
pub fn list_threads(
    state: State<'_, AppState>,
    view: View,
    cursor: Option<Cursor>,
    limit: u32,
) -> Result<ThreadPage, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    read::list_threads(&db, &view, cursor, limit).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn get_thread(
    state: State<'_, AppState>,
    thread_id: i64,
) -> Result<Option<ThreadDetail>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    read::get_thread(&db, thread_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn get_body(state: State<'_, AppState>, message_id: i64) -> Result<Option<Body>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    read::get_body(&db, message_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    read::list_accounts(&db).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn list_labels(
    state: State<'_, AppState>,
    account_id: Option<i64>,
) -> Result<Vec<Label>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    read::list_labels(&db, account_id).map_err(|e| e.to_string())
}

/// Called by the UI after its first populated frame. Shows the window and
/// returns the cold-launch time in ms (process start to this call).
#[tauri::command]
#[specta]
pub fn report_first_paint(window: tauri::WebviewWindow) -> Result<u32, String> {
    let elapsed = crate::ms_since_start().round() as u32;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    eprintln!("tidemail: cold launch {elapsed} ms");
    Ok(elapsed)
}

#[tauri::command(async)]
#[specta]
pub fn apply_action(
    state: State<'_, AppState>,
    request: ActionRequest,
) -> Result<ActionTicket, String> {
    state
        .engine
        .apply_action(request)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn undo_action(state: State<'_, AppState>, action_ids: Vec<i64>) -> Result<(), String> {
    state
        .engine
        .undo_action(&action_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn retry_failed(state: State<'_, AppState>, action_id: i64) -> Result<(), String> {
    state
        .engine
        .retry_failed(action_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn dismiss_failed(state: State<'_, AppState>, action_id: i64) -> Result<(), String> {
    state
        .engine
        .dismiss_failed(action_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn search(
    state: State<'_, AppState>,
    query: String,
    limit: u32,
) -> Result<Vec<SearchHit>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    tidemail_search::search(&db, &query, limit).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn get_sync_status(state: State<'_, AppState>) -> SyncStatus {
    state.engine.status()
}

#[tauri::command]
#[specta]
pub async fn add_account(state: State<'_, AppState>) -> Result<Account, String> {
    state.engine.add_account().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta]
pub async fn remove_account(state: State<'_, AppState>, account_id: i64) -> Result<(), String> {
    state
        .engine
        .remove_account(account_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta]
pub fn prefetch_bodies(state: State<'_, AppState>, message_ids: Vec<i64>) {
    state.engine.prefetch_bodies(&message_ids);
}

#[tauri::command]
#[specta]
pub async fn download_attachment(
    state: State<'_, AppState>,
    attachment_id: i64,
) -> Result<String, String> {
    state
        .engine
        .download_attachment(attachment_id)
        .await
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}
