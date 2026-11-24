//! Status area: "Synced 12 s ago", "Offline", "3 actions pending",
//! "Sending 1 message", "1 message not sent" with Retry / Edit, and
//! "1 action failed" with Retry / Dismiss. The shell repaints it every few
//! seconds so the age stays current.

use std::time::Duration;

use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::types::{AccountSyncState, SyncStatus};

use crate::{format, state::AppState, theme::colors, ui};

/// How often the shell repaints for the "Synced N s ago" text.
pub const TICK: Duration = Duration::from_secs(5);

fn plural(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// The status sentence. `now_ms` is the current time in epoch ms.
pub fn text(s: &SyncStatus, now_ms: i64) -> String {
    let any = |state| s.accounts.iter().any(|a| a.state == state);
    if !s.failed_sends.is_empty() {
        format!("{} not sent", plural(s.failed_sends.len(), "message"))
    } else if !s.failed_actions.is_empty() {
        format!("{} failed", plural(s.failed_actions.len(), "action"))
    } else if s.queued_offline > 0 {
        format!("{} queued (offline)", plural(s.queued_offline as usize, "message"))
    } else if s.pending_sends > 0 {
        format!("Sending {}", plural(s.pending_sends as usize, "message"))
    } else if s.pending_actions > 0 {
        format!("{} pending", plural(s.pending_actions as usize, "action"))
    } else if any(AccountSyncState::Offline) {
        "Offline".into()
    } else if any(AccountSyncState::AuthError) {
        "Sign-in needed".into()
    } else if let Some(b) = s
        .accounts
        .iter()
        .filter_map(|a| a.backfill)
        .min_by_key(|b| b.percent)
    {
        // The account furthest behind.
        let what = if b.older { "Downloading older mail" } else { "First sync" };
        format!("{what} {} % · needs internet", b.percent)
    } else if any(AccountSyncState::Syncing) {
        "Syncing…".into()
    } else {
        match s.accounts.iter().filter_map(|a| a.last_sync_at).max() {
            Some(at) if now_ms - at < 5_000 => "Synced just now".into(),
            Some(at) => format!("Synced {} ago", format::age(now_ms - at)),
            None => "Not synced yet".into(),
        }
    }
}

#[derive(Clone, Copy)]
enum Fix {
    RetryActions,
    DismissActions,
    RetrySends,
    /// Open the first failed message in compose (it leaves the queue).
    EditSend,
}

/// One status line. With failed sends it shows Retry and Edit; with failed
/// actions Retry and Dismiss. Retry and Dismiss call the engine for every
/// failed item (the engine reports the new status).
pub fn line(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let st = state.read(cx);
    let s = &st.status;
    let sends: Vec<i64> = s.failed_sends.iter().map(|f| f.id).collect();
    let actions: Vec<i64> = s.failed_actions.iter().map(|f| f.id).collect();
    let engine = st.engine.clone();
    let button = |id: &'static str, label: &'static str, fix: Fix| {
        let (engine, state) = (engine.clone(), state.clone());
        let ids = match fix {
            Fix::RetrySends | Fix::EditSend => sends.clone(),
            Fix::RetryActions | Fix::DismissActions => actions.clone(),
        };
        div()
            .id(id)
            .px_1()
            .rounded(px(4.))
            .cursor_pointer()
            .text_color(c.accent)
            .hover(|d| d.bg(c.hover))
            .child(label)
            .on_click(move |_, _, cx| {
                if let (Fix::EditSend, Some(&id)) = (fix, ids.first()) {
                    state.update(cx, |s, cx| s.take_back_send(id, cx));
                    return;
                }
                let (engine, ids) = (engine.clone(), ids.clone());
                cx.background_spawn(async move {
                    for id in ids {
                        let result = match fix {
                            Fix::RetryActions => engine.retry_failed(id),
                            Fix::DismissActions => engine.dismiss_failed(id),
                            Fix::RetrySends => engine.retry_send(id),
                            Fix::EditSend => Ok(()),
                        };
                        if let Err(e) = result {
                            eprintln!("tidemail: retry {id}: {e}");
                        }
                    }
                })
                .detach();
            })
    };
    let failed = !sends.is_empty() || !actions.is_empty();
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_size(px(ui::TEXT_XS))
        .text_color(if failed { c.danger } else { c.muted_fg })
        .child(text(s, tidemail_db::now_ms()))
        .map(|d| match (sends.is_empty(), actions.is_empty()) {
            (false, _) => d
                .child(button("status-retry", "Retry", Fix::RetrySends))
                .child(button("status-edit", "Edit", Fix::EditSend)),
            (true, false) => d
                .child(button("status-retry", "Retry", Fix::RetryActions))
                .child(button("status-dismiss", "Dismiss", Fix::DismissActions)),
            (true, true) => d,
        })
}

#[cfg(test)]
mod tests {
    use tidemail_db::types::{
        AccountSync, AccountSyncState, ActionKind, Backfill, FailedAction, SyncStatus,
    };

    use super::text;

    fn account(state: AccountSyncState, at: Option<i64>) -> AccountSync {
        AccountSync {
            account_id: 1,
            state,
            last_sync_at: at,
            sync_from: None,
            backfill: None,
        }
    }

    fn status(accounts: Vec<AccountSync>, pending: u32, failed: usize) -> SyncStatus {
        SyncStatus {
            accounts,
            pending_actions: pending,
            failed_actions: (0..failed)
                .map(|i| FailedAction {
                    id: i as i64,
                    thread_id: 1,
                    kind: ActionKind::Archive,
                    error: "x".into(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn sends_come_before_actions() {
        let off = account(AccountSyncState::Offline, Some(1));
        let mut s = status(vec![off], 2, 1);
        s.pending_sends = 1;
        assert_eq!(text(&s, 9), "1 action failed");
        s.failed_actions.clear();
        assert_eq!(text(&s, 9), "Sending 1 message");
        s.queued_offline = 1;
        assert_eq!(text(&s, 9), "1 message queued (offline)");
        s.failed_sends.push(tidemail_db::types::FailedSend {
            id: 1,
            subject: "Hi".into(),
            error: "x".into(),
        });
        assert_eq!(text(&s, 9), "1 message not sent");
    }

    #[test]
    fn synced_age() {
        let s = status(vec![account(AccountSyncState::Idle, Some(100_000))], 0, 0);
        assert_eq!(text(&s, 102_000), "Synced just now");
        assert_eq!(text(&s, 112_000), "Synced 12 s ago");
        assert_eq!(text(&s, 100_000 + 5 * 60_000), "Synced 5 min ago");
    }

    #[test]
    fn newest_account_counts() {
        let s = status(
            vec![
                account(AccountSyncState::Idle, Some(10_000)),
                account(AccountSyncState::Idle, Some(50_000)),
            ],
            0,
            0,
        );
        assert_eq!(text(&s, 60_000), "Synced 10 s ago");
    }

    #[test]
    fn priority_failed_pending_offline() {
        let off = account(AccountSyncState::Offline, Some(1));
        assert_eq!(text(&status(vec![off.clone()], 0, 0), 9), "Offline");
        assert_eq!(
            text(&status(vec![off.clone()], 3, 0), 9),
            "3 actions pending"
        );
        assert_eq!(
            text(&status(vec![off.clone()], 1, 0), 9),
            "1 action pending"
        );
        assert_eq!(text(&status(vec![off.clone()], 3, 1), 9), "1 action failed");
        assert_eq!(text(&status(vec![off], 0, 2), 9), "2 actions failed");
    }

    #[test]
    fn backfill_shows_the_account_furthest_behind() {
        let mut a = account(AccountSyncState::Syncing, None);
        a.backfill = Some(Backfill {
            percent: 40,
            older: false,
        });
        let mut b = account(AccountSyncState::Idle, Some(1));
        b.backfill = Some(Backfill {
            percent: 10,
            older: false,
        });
        let s = status(vec![a.clone(), b.clone()], 0, 0);
        assert_eq!(text(&s, 9), "First sync 10 % · needs internet");
        b.backfill = Some(Backfill {
            percent: 10,
            older: true,
        });
        let s = status(vec![a.clone(), b], 0, 0);
        assert_eq!(text(&s, 9), "Downloading older mail 10 % · needs internet");
        let mut off = a;
        off.state = AccountSyncState::Offline;
        assert_eq!(text(&status(vec![off], 0, 0), 9), "Offline");
    }

    #[test]
    fn never_synced() {
        let s = status(vec![account(AccountSyncState::Idle, None)], 0, 0);
        assert_eq!(text(&s, 9), "Not synced yet");
    }
}
