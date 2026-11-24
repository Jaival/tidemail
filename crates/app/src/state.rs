//! Shared app state: one entity per window. Frozen for Phase 1G-B; changes
//! go through docs/HANDOFF.md.
//!
//! Views read fields directly and call methods to change them. Every change
//! emits a [`StateEvent`]; views subscribe to the ones they need.

use std::{path::PathBuf, sync::Arc, time::Duration};

use gpui_kit::{App, AppContext, Context, EventEmitter, SharedString, Task, Window};
use tidemail_db::{
    read,
    types::{
        Account, ActionKind, ActionRequest, Label, OutgoingMail, SyncStatus, ThreadRow,
        UnreadCount, View,
    },
};
use tidemail_sync::Engine;

use crate::{
    data::{self, Db},
    format,
    preset::{Preset, PresetSpec},
    settings::{Settings, ThemePref},
};

/// A list in the sidebar: a label, unified or for one account.
#[derive(Debug, Clone, PartialEq)]
pub struct Nav {
    pub view: View,
    pub title: SharedString,
}

impl Nav {
    pub fn inbox() -> Self {
        Self {
            view: data::inbox(),
            title: "Inbox".into(),
        }
    }
}

/// Something drawn over the panes. While any is open the reader hides its
/// webview (the OS draws it above GPUI).
#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    Palette,
    LabelPicker {
        thread_id: i64,
        account_id: i64,
    },
    MovePicker {
        thread_id: i64,
        account_id: i64,
    },
    /// "Go to label": every label, for tabs and the rail (`g l`).
    GoPicker,
    /// Theme, accent and fonts (`features::appearance`); `fonts` opens the Fonts tab.
    Appearance { fonts: bool },
    /// How much mail each account keeps; download older mail (`features::accounts`).
    Accounts,
    /// Not pushed: toasts sit in the list pane or hint bar, never over the webview (ADR 0002).
    #[allow(dead_code)]
    Toast,
}

#[derive(Debug, Clone)]
pub enum StateEvent {
    NavChanged,
    /// The current thread changed (`AppState::selected`).
    SelectionChanged,
    /// The reader opened or closed (`AppState::reading`).
    ReadingChanged,
    /// Rows changed in the database (sync or a local action).
    DataChanged {
        thread_ids: Vec<i64>,
        label_keys: Vec<String>,
    },
    AccountsChanged,
    StatusChanged,
    OverlaysChanged,
    SettingsChanged,
    /// A user action was applied locally. `action_ids` undo it.
    ActionDone {
        text: SharedString,
        action_ids: Vec<i64>,
    },
    ActionFailed {
        text: SharedString,
    },
    /// Information for a toast without undo (sign-in progress).
    Notice {
        text: SharedString,
    },
    /// Compose opened or closed (`AppState::composing`).
    ComposeChanged,
    /// A message went into the send queue; `outbox_id` undoes it.
    SendQueued {
        outbox_id: i64,
    },
    /// Open compose with this message (undo send, edit a failed send).
    ReopenCompose(OutgoingMail),
}

pub struct AppState {
    pub db: Arc<Db>,
    pub engine: Arc<Engine>,
    pub accounts: Vec<Account>,
    pub labels: Vec<Label>,
    /// Unread threads per account and label; reloaded with `labels`.
    pub unread: Vec<UnreadCount>,
    pub nav: Nav,
    /// Current thread: selected in the list, shown by the reader.
    pub selected: Option<i64>,
    /// The reader is on screen. With `ReaderPlacement::Replace` it replaces the list.
    pub reading: bool,
    /// A search query is active: its results replace the thread list.
    pub searching: bool,
    /// Compose is open in the reader's place.
    pub composing: bool,
    pub status: SyncStatus,
    pub settings: Settings,
    settings_path: PathBuf,
    overlays: Vec<Overlay>,
    labels_reload: Option<Task<()>>,
}

impl EventEmitter<StateEvent> for AppState {}

impl AppState {
    pub fn new(
        db: Arc<Db>,
        engine: Arc<Engine>,
        accounts: Vec<Account>,
        labels: Vec<Label>,
        unread: Vec<UnreadCount>,
        settings_path: PathBuf,
        preset_override: Option<Preset>,
    ) -> Self {
        let status = engine.status();
        let mut settings = Settings::load(&settings_path);
        if let Some(p) = preset_override {
            settings.preset = p;
        }
        Self {
            db,
            engine,
            accounts,
            labels,
            unread,
            nav: Nav::inbox(),
            selected: None,
            reading: false,
            searching: false,
            composing: false,
            status,
            settings,
            settings_path,
            overlays: Vec::new(),
            labels_reload: None,
        }
    }

    pub fn account(&self, id: i64) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    /// Unread threads in a view (summed over accounts when unified).
    pub fn unread(&self, view: &View) -> i64 {
        self.unread
            .iter()
            .filter(|u| u.label_key == view.label_key)
            .filter(|u| view.account_id.is_none_or(|a| a == u.account_id))
            .map(|u| u.count)
            .sum()
    }

    pub fn spec(&self) -> &'static PresetSpec {
        self.settings.preset.spec()
    }

    pub fn navigate(&mut self, nav: Nav, cx: &mut Context<Self>) {
        if self.nav != nav {
            self.nav = nav;
            self.selected = None;
            cx.emit(StateEvent::NavChanged);
            cx.emit(StateEvent::SelectionChanged);
            self.close_reader(cx);
        }
    }

    /// Make a thread current without opening it (no mark read).
    pub fn select(&mut self, thread: &ThreadRow, cx: &mut Context<Self>) {
        if self.selected != Some(thread.id) {
            self.selected = Some(thread.id);
            cx.emit(StateEvent::SelectionChanged);
        }
    }

    /// Open a thread: select it, show the reader, mark it read (silent: no
    /// toast, no undo).
    pub fn open(&mut self, thread: &ThreadRow, cx: &mut Context<Self>) {
        self.select(thread, cx);
        if !self.reading {
            self.reading = true;
            cx.emit(StateEvent::ReadingChanged);
        }
        if thread.unread_count > 0 {
            self.apply(ActionKind::MarkRead, vec![thread.id], None, cx);
        }
    }

    /// Leave the reader; the selection stays.
    pub fn close_reader(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            self.reading = false;
            cx.emit(StateEvent::ReadingChanged);
        }
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.selected.take().is_some() {
            cx.emit(StateEvent::SelectionChanged);
        }
        self.close_reader(cx);
    }

    /// Apply an action locally now (the engine sends it to Gmail later).
    /// With `text`, emits `ActionDone` for the undo toast.
    pub fn apply(
        &mut self,
        kind: ActionKind,
        thread_ids: Vec<i64>,
        text: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let engine = self.engine.clone();
        let req = ActionRequest { thread_ids, kind };
        let task = cx.background_spawn(async move { engine.apply_action(req) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |_, cx| match (result, text) {
                (Ok(ticket), Some(text)) => cx.emit(StateEvent::ActionDone {
                    text,
                    action_ids: ticket.action_ids,
                }),
                (Ok(_), None) => {}
                (Err(e), text) => cx.emit(StateEvent::ActionFailed {
                    text: format!(
                        "{} failed. {}",
                        text.unwrap_or("Action".into()),
                        format::user_error(&e)
                    )
                    .into(),
                }),
            });
        })
        .detach();
    }

    pub fn undo(&mut self, action_ids: Vec<i64>, cx: &mut Context<Self>) {
        let engine = self.engine.clone();
        let task = cx.background_spawn(async move { engine.undo_action(&action_ids) });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                let _ = this.update(cx, |_, cx| {
                    cx.emit(StateEvent::ActionFailed {
                        text: format!("Undo failed. {}", format::user_error(&e)).into(),
                    })
                });
            }
        })
        .detach();
    }

    pub fn set_composing(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.composing != on {
            self.composing = on;
            cx.emit(StateEvent::ComposeChanged);
        }
    }

    /// Account a new message is sent from: the one of the current view, else
    /// the first.
    pub fn compose_account(&self) -> Option<i64> {
        self.nav
            .view
            .account_id
            .filter(|id| self.account(*id).is_some())
            .or_else(|| self.accounts.first().map(|a| a.id))
    }

    /// Queue a message (sent after the 10 s undo window). On failure the
    /// message comes back in compose.
    pub fn send(&mut self, mail: OutgoingMail, cx: &mut Context<Self>) {
        let engine = self.engine.clone();
        let task = cx.background_spawn({
            let mail = mail.clone();
            async move { engine.queue_send(mail) }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |_, cx| match result {
                Ok(ticket) => cx.emit(StateEvent::SendQueued {
                    outbox_id: ticket.outbox_id,
                }),
                Err(e) => {
                    cx.emit(StateEvent::ActionFailed {
                        text: format!("Not sent. {}", format::user_error(&e)).into(),
                    });
                    cx.emit(StateEvent::ReopenCompose(mail));
                }
            });
        })
        .detach();
    }

    /// Take a queued message back (Undo send, or Edit after a failure) and
    /// open it in compose.
    pub fn take_back_send(&mut self, outbox_id: i64, cx: &mut Context<Self>) {
        let engine = self.engine.clone();
        let task = cx.background_spawn(async move { engine.cancel_send(outbox_id) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |_, cx| match result {
                Ok(Some(mail)) => cx.emit(StateEvent::ReopenCompose(mail)),
                Ok(None) => cx.emit(StateEvent::Notice {
                    text: "Too late to undo: the message is on its way".into(),
                }),
                Err(e) => cx.emit(StateEvent::ActionFailed {
                    text: format!("Undo failed. {}", format::user_error(&e)).into(),
                }),
            });
        })
        .detach();
    }

    /// Fetch missing bodies of these threads (the next N in the list) in the background.
    pub fn prefetch(&self, thread_ids: Vec<i64>, cx: &App) {
        if thread_ids.is_empty() {
            return;
        }
        let engine = self.engine.clone();
        self.db
            .read(cx, move |conn| {
                let mut st = conn.prepare_cached(
                    "SELECT id FROM messages WHERE thread_id = ?1 AND body_state != 1",
                )?;
                let mut ids = Vec::new();
                for t in thread_ids {
                    for id in st.query_map([t], |r| r.get(0))? {
                        ids.push(id?);
                    }
                }
                engine.prefetch_bodies(&ids);
                Ok(())
            })
            .detach();
    }

    pub fn overlay_open(&self) -> bool {
        !self.overlays.is_empty()
    }

    pub fn top_overlay(&self) -> Option<&Overlay> {
        self.overlays.last()
    }

    pub fn push_overlay(&mut self, overlay: Overlay, cx: &mut Context<Self>) {
        self.overlays.push(overlay);
        cx.emit(StateEvent::OverlaysChanged);
    }

    pub fn close_overlay(&mut self, overlay: &Overlay, cx: &mut Context<Self>) {
        let before = self.overlays.len();
        self.overlays.retain(|o| o != overlay);
        if self.overlays.len() != before {
            cx.emit(StateEvent::OverlaysChanged);
        }
    }

    pub fn set_theme(&mut self, theme: ThemePref, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.theme = theme;
        crate::theme::apply(&self.settings, window, cx);
        self.save_settings(cx);
    }

    /// Switch layout (and the palette, with "Preset colours") live.
    pub fn set_preset(&mut self, preset: Preset, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.preset != preset {
            self.settings.preset = preset;
            crate::theme::apply(&self.settings, window, cx);
            self.save_settings(cx);
        }
    }

    /// Replace the settings (the Appearance overlay: unsaved while it
    /// previews, saved on Done). The caller has applied the theme already.
    pub fn set_settings(&mut self, settings: Settings, save: bool, cx: &mut Context<Self>) {
        self.settings = settings;
        if save {
            self.save_settings(cx);
        } else {
            cx.emit(StateEvent::SettingsChanged);
        }
    }

    /// Font for subjects and list titles: the preset's display font, unless
    /// the interface font is used for headings too.
    pub fn display_font(&self) -> Option<&'static str> {
        if self.settings.headings_use_ui_font {
            None
        } else {
            self.spec().display_font
        }
    }

    pub fn set_prefetch(&mut self, n: u32, cx: &mut Context<Self>) {
        self.settings.prefetch = n;
        self.save_settings(cx);
    }

    fn save_settings(&self, cx: &mut Context<Self>) {
        let (settings, path) = (self.settings.clone(), self.settings_path.clone());
        cx.background_spawn(async move {
            if let Err(e) = settings.save(&path) {
                eprintln!("tidemail: saving settings failed: {e}");
            }
        })
        .detach();
        cx.emit(StateEvent::SettingsChanged);
    }

    pub fn on_data_changed(
        &mut self,
        thread_ids: Vec<i64>,
        label_keys: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(StateEvent::DataChanged {
            thread_ids,
            label_keys,
        });
        // Bursts during backfill: reload accounts, labels and unread counts once per burst.
        if self.labels_reload.is_none() {
            let db = self.db.clone();
            self.labels_reload = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let loaded = cx
                    .background_spawn(async move {
                        db.with(|c| {
                            Ok((
                                read::list_accounts(c)?,
                                read::list_labels(c, None)?,
                                read::unread_counts(c)?,
                            ))
                        })
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.labels_reload = None;
                    if let Ok((accounts, labels, unread)) = loaded {
                        this.accounts = accounts;
                        this.labels = labels;
                        this.unread = unread;
                        cx.emit(StateEvent::AccountsChanged);
                    }
                });
            }));
        }
    }

    /// Browser sign-in for a new Gmail account; its backfill starts after.
    pub fn add_account(&mut self, cx: &mut Context<Self>) {
        cx.emit(StateEvent::Notice {
            text: "Sign in to Google in your browser…".into(),
        });
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let result = engine.add_account().await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(account) => {
                    cx.emit(StateEvent::Notice {
                        text: format!(
                            "Added {}. Tidemail downloads the last 6 months of mail. \
                             This can take a while for a big mailbox. It runs while \
                             Tidemail is open or in the tray, and needs internet. For \
                             older mail, open Accounts and sync (Ctrl+K).",
                            account.email
                        )
                        .into(),
                    });
                    this.reload_accounts(cx);
                }
                Err(e) => cx.emit(StateEvent::ActionFailed {
                    text: format!("Sign-in failed. {}", format::user_error(&e)).into(),
                }),
            });
        })
        .detach();
    }

    /// Remove the account, its local mail and its stored sign-in token.
    pub fn sign_out(&mut self, account_id: i64, cx: &mut Context<Self>) {
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let result = engine.remove_account(account_id).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => {
                    cx.emit(StateEvent::Notice {
                        text: "Signed out".into(),
                    });
                    this.reload_accounts(cx);
                }
                Err(e) => cx.emit(StateEvent::ActionFailed {
                    text: format!("Sign-out failed. {}", format::user_error(&e)).into(),
                }),
            });
        })
        .detach();
    }

    /// Reload accounts, labels and unread counts now (no sync event follows).
    fn reload_accounts(&mut self, cx: &mut Context<Self>) {
        self.on_data_changed(Vec::new(), Vec::new(), cx);
    }

    pub fn on_status(&mut self, status: SyncStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.emit(StateEvent::StatusChanged);
    }
}
