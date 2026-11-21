//! Navigation model, drawn by `sidebar`, `tabs` and `rail`.

use gpui_kit::{Hsla, SharedString};
use tidemail_db::types::View;

use crate::{
    format,
    state::{AppState, Nav},
};

/// Unified mailboxes, and each account's sub-mailboxes (first one is the inbox).
pub const MAILBOXES: [(&str, &str, &str); 4] = [
    ("INBOX", "Inbox", "icons/inbox.svg"),
    ("STARRED", "Starred", "icons/star.svg"),
    ("SENT", "Sent", "icons/send.svg"),
    ("DRAFT", "Drafts", "icons/file-pen-line.svg"),
];

/// Only these show an unread count (as Gmail does).
fn counts_unread(label_key: &str) -> bool {
    label_key == "INBOX" || label_key.starts_with("u:")
}

pub struct Item {
    pub nav: Nav,
    pub icon: Option<&'static str>,
    pub color: Option<Hsla>,
    pub unread: i64,
    pub active: bool,
}

pub struct AccountItem {
    pub item: Item,
    /// Starred, Sent, Drafts of this account; filled while it is the current account.
    pub sub: Vec<Item>,
}

pub struct Items {
    pub mailboxes: Vec<Item>,
    pub accounts: Vec<AccountItem>,
    pub labels: Vec<Item>,
}

pub fn nav(label_key: &str, account_id: Option<i64>, title: impl Into<SharedString>) -> Nav {
    Nav {
        view: View {
            label_key: label_key.into(),
            account_id,
        },
        title: title.into(),
    }
}

/// The current view limited to one account, or unified again if it already is.
/// Account dots in tabs and the rail use it.
pub fn toggle_account(s: &AppState, account_id: i64) -> Nav {
    let cur = &s.nav.view;
    let account_id = (cur.account_id != Some(account_id)).then_some(account_id);
    // A user label belongs to one account: switching account goes to its inbox.
    let key = if cur.label_key.starts_with("u:") && account_id.is_some() {
        "INBOX"
    } else {
        &cur.label_key
    };
    let title = MAILBOXES
        .iter()
        .find(|(k, ..)| *k == key)
        .map_or_else(|| s.nav.title.clone(), |(_, t, _)| (*t).into());
    nav(key, account_id, title)
}

pub fn items(s: &AppState) -> Items {
    let current = &s.nav.view;
    let item = |nav: Nav, icon, color| {
        let unread = if counts_unread(&nav.view.label_key) {
            s.unread(&nav.view)
        } else {
            0
        };
        Item {
            active: nav.view == *current,
            nav,
            icon,
            color,
            unread,
        }
    };
    let mailboxes = MAILBOXES
        .iter()
        .map(|(k, t, i)| item(nav(k, None, *t), Some(*i), None))
        .collect();
    let accounts = s
        .accounts
        .iter()
        .map(|a| {
            let name = a.display_name.clone().unwrap_or_else(|| a.email.clone());
            let color = Some(format::parse_color(&a.color));
            let sub = if current.account_id == Some(a.id) {
                MAILBOXES[1..]
                    .iter()
                    .map(|(k, t, i)| item(nav(k, Some(a.id), *t), Some(*i), None))
                    .collect()
            } else {
                Vec::new()
            };
            AccountItem {
                item: item(nav("INBOX", Some(a.id), name), None, color),
                sub,
            }
        })
        .collect();
    let labels = s
        .labels
        .iter()
        .filter(|l| !l.is_system)
        .map(|l| {
            let color = s
                .account(l.account_id)
                .map(|a| format::parse_color(&a.color));
            item(
                nav(&l.label_key, Some(l.account_id), l.name.clone()),
                None,
                color,
            )
        })
        .collect();
    Items {
        mailboxes,
        accounts,
        labels,
    }
}
