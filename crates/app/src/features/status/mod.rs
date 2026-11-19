//! Status area. Phase 1G-A stub: one line from `SyncStatus`. Phase 1G-B
//! (commands agent) adds "Synced 12 s ago", Retry / Dismiss.

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use tidemail_db::types::AccountSyncState;

use crate::state::AppState;

pub fn line(state: &AppState, cx: &App) -> impl IntoElement {
    let s = &state.status;
    let text = if !s.failed_actions.is_empty() {
        format!("{} action(s) failed", s.failed_actions.len())
    } else if s.pending_actions > 0 {
        format!("{} action(s) pending", s.pending_actions)
    } else if s
        .accounts
        .iter()
        .any(|a| a.state == AccountSyncState::Offline)
    {
        "Offline".into()
    } else if s
        .accounts
        .iter()
        .any(|a| a.state == AccountSyncState::Syncing)
    {
        "Syncing…".into()
    } else {
        "Up to date".into()
    };
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}
