//! Window content: title bar with the search slot, sidebar, thread list, reader.
//! Owns keyboard focus and routes actions.

pub mod nav;
mod rail;
mod sidebar;
mod tabs;

use gpui_kit::component::{ActiveTheme, TitleBar};
use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::types::{ActionKind, ThreadPage};

use crate::{
    actions::*,
    features::{
        list::ThreadList,
        palette::{self, toast::Toast, Picker},
        reader::Reader,
        search::{Results, Search, SearchEvent},
        status,
    },
    preset::{NavStyle, ReaderPlacement},
    state::{AppState, Nav, Overlay, StateEvent},
    theme::colors,
    ui,
};

pub struct Shell {
    pub state: Entity<AppState>,
    pub list: Entity<ThreadList>,
    pub reader: Entity<Reader>,
    pub search: Entity<Search>,
    focus: FocusHandle,
    /// The picker for the top overlay.
    picker: Option<(Overlay, Entity<Picker>)>,
    toast: Option<Toast>,
    toast_timer: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(
        state: Entity<AppState>,
        first: ThreadPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = cx.new(|cx| ThreadList::new(state.clone(), first, cx));
        let reader = cx.new(|cx| Reader::new(state.clone(), window, cx));
        let search = cx.new(|cx| Search::new(state.clone(), window, cx));
        let sub = cx.subscribe_in(&state, window, |this, _, event, window, cx| match event {
            StateEvent::NavChanged
            | StateEvent::AccountsChanged
            | StateEvent::StatusChanged
            | StateEvent::ReadingChanged
            | StateEvent::SettingsChanged => cx.notify(),
            StateEvent::OverlaysChanged => this.sync_picker(window, cx),
            StateEvent::ActionDone { text, action_ids } => this.show_toast(
                Toast {
                    text: text.clone(),
                    undo: Some(action_ids.clone()),
                },
                cx,
            ),
            StateEvent::ActionFailed { text } => this.show_toast(
                Toast {
                    text: text.clone(),
                    undo: None,
                },
                cx,
            ),
            _ => {}
        });
        // The "Synced N s ago" text ages: repaint now and then.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(status::TICK).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // The input hands focus back (Escape, Enter) so j/k work again.
        let leave = cx.subscribe_in(&search, window, |this, _, _: &SearchEvent, window, cx| {
            this.focus.focus(window, cx)
        });
        // Show or hide the results list when the query changes.
        let observe = cx.observe(&search, |_, _, cx| cx.notify());
        Self {
            state,
            list,
            reader,
            search,
            focus,
            picker: None,
            toast: None,
            toast_timer: None,
            _subscriptions: vec![sub, leave, observe],
        }
    }

    /// Open or close the picker to match the top overlay; focus goes back to
    /// the shell when no picker is left.
    fn sync_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let top = self.state.read(cx).top_overlay().cloned();
        if self.picker.as_ref().map(|(o, _)| o) == top.as_ref() {
            return;
        }
        self.picker =
            top.and_then(|o| palette::open(&o, &self.state, window, cx).map(|picker| (o, picker)));
        if self.picker.is_none() {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    /// Show a toast for 5 s (replaces the current one).
    fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toast = Some(toast);
        self.toast_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(palette::toast::LIFETIME)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.toast = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Undo the action of the current toast (Ctrl+Z or the button).
    fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(Toast {
            undo: Some(ids), ..
        }) = self.toast.take()
        {
            self.toast_timer = None;
            self.state.update(cx, |s, cx| s.undo(ids, cx));
            cx.notify();
        }
    }

    /// Esc: close the top overlay. False if none is open.
    fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(top) = self.state.read(cx).top_overlay().cloned() else {
            return false;
        };
        self.state.update(cx, |s, cx| s.close_overlay(&top, cx));
        true
    }

    fn push_overlay(&mut self, overlay: Overlay, cx: &mut Context<Self>) {
        if self.state.read(cx).top_overlay() != Some(&overlay) {
            self.state.update(cx, |s, cx| s.push_overlay(overlay, cx));
        }
    }

    /// Open a thread picker (label, move) for the current thread.
    fn pick_for_thread(
        &mut self,
        overlay: impl FnOnce(i64, i64) -> Overlay,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.current_row(cx) {
            self.push_overlay(overlay(row.id, row.account_id), cx);
        }
    }

    /// The results list, while a query is active.
    fn results(&self, cx: &App) -> Option<Entity<Results>> {
        let s = self.search.read(cx);
        s.active.then(|| s.results.clone())
    }

    /// Current row of the search results while a query is active, else of the list.
    fn current_row(&self, cx: &App) -> Option<tidemail_db::types::ThreadRow> {
        match self.results(cx) {
            Some(r) => r.read(cx).current(cx),
            None => self.list.read(cx).current(cx),
        }
    }

    /// Apply `kind` to the opened thread, with an undo toast.
    fn act(
        &mut self,
        kind: impl FnOnce(&tidemail_db::types::ThreadRow) -> (ActionKind, &'static str),
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.current_row(cx) else {
            return;
        };
        let (kind, text) = kind(&row);
        self.state.update(cx, |s, cx| {
            s.apply(kind, vec![row.id], Some(text.into()), cx)
        });
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let spec = self.state.read(cx).spec();
        let mut context = KeyContext::default();
        // While the search input has focus the single-key bindings must not
        // match: the shell context is replaced by one without bindings.
        context.add(if self.search.read(cx).focused {
            "TidemailSearch"
        } else {
            CONTEXT
        });
        if self.state.read(cx).reading {
            context.add(READING);
        }
        div()
            .id("shell")
            .key_context(context)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .text_size(theme.font_size)
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                match this.results(cx) {
                    Some(r) => r.update(cx, |r, cx| r.select_next(cx)),
                    None => this.list.update(cx, |l, cx| l.select_next(cx)),
                }
            }))
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| {
                match this.results(cx) {
                    Some(r) => r.update(cx, |r, cx| r.select_prev(cx)),
                    None => this.list.update(cx, |l, cx| l.select_prev(cx)),
                }
            }))
            .on_action(cx.listener(|this, _: &OpenThread, _, cx| {
                match this.results(cx) {
                    Some(r) => r.update(cx, |r, cx| r.open_current(cx)),
                    None => this.list.update(cx, |l, cx| l.open_current(cx)),
                }
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.update(cx, |s, cx| s.focus(window, cx))
            }))
            .on_action(cx.listener(|this, _: &CloseReader, _, cx| {
                // Esc with an overlay open closes the overlay, not the reader.
                if !this.dismiss(cx) {
                    this.state.update(cx, |s, cx| s.close_reader(cx))
                }
            }))
            // Esc: close the top overlay, else clear an active search query.
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                if !this.dismiss(cx) && this.results(cx).is_some() {
                    this.search.update(cx, |s, cx| s.clear(window, cx))
                }
            }))
            .on_action(cx.listener(|this, _: &CommandPalette, _, cx| {
                match this.state.read(cx).top_overlay() == Some(&Overlay::Palette) {
                    true => this.dismiss(cx),
                    false => {
                        this.push_overlay(Overlay::Palette, cx);
                        true
                    }
                };
            }))
            .on_action(cx.listener(|this, _: &PickLabel, _, cx| {
                this.pick_for_thread(
                    |thread_id, account_id| Overlay::LabelPicker {
                        thread_id,
                        account_id,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &PickMove, _, cx| {
                this.pick_for_thread(
                    |thread_id, account_id| Overlay::MovePicker {
                        thread_id,
                        account_id,
                    },
                    cx,
                )
            }))
            .on_action(
                cx.listener(|this, _: &GoLabel, _, cx| this.push_overlay(Overlay::GoPicker, cx)),
            )
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.undo(cx)))
            .on_action(cx.listener(|this, _: &GoInbox, _, cx| {
                this.state.update(cx, |s, cx| s.navigate(Nav::inbox(), cx))
            }))
            .on_action(cx.listener(|this, _: &Archive, _, cx| {
                this.act(|_| (ActionKind::Archive, "Archived"), cx)
            }))
            .on_action(cx.listener(|this, _: &Trash, _, cx| {
                this.act(|_| (ActionKind::Trash, "Moved to Trash"), cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleStar, _, cx| {
                this.act(
                    |r| match r.is_starred {
                        true => (ActionKind::Unstar, "Unstarred"),
                        false => (ActionKind::Star, "Starred"),
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &ToggleRead, _, cx| {
                this.act(
                    |r| match r.unread_count > 0 {
                        true => (ActionKind::MarkRead, "Marked read"),
                        false => (ActionKind::MarkUnread, "Marked unread"),
                    },
                    cx,
                )
            }))
            .relative()
            .child(self.title_bar(cx))
            .child(self.body(cx))
            .when(spec.hint_bar, |d| d.child(self.hint_bar(cx)))
            .when_some(self.picker.as_ref(), |d, (_, p)| d.child(p.clone()))
    }
}

impl Shell {
    fn toast_content(&self, cx: &Context<Self>) -> Option<Div> {
        let toast = self.toast.as_ref()?;
        Some(palette::toast::content(
            toast,
            cx.listener(|this, _, _, cx| this.undo(cx)),
            cx,
        ))
    }

    fn title_bar(&self, cx: &App) -> impl IntoElement {
        let s = self.state.read(cx);
        let c = colors(cx);
        // Command: the input replaces the tabs while it has focus or a query is active.
        let search_open = self.search.read(cx).active || self.search.read(cx).focused;
        let tabs = s.spec().nav == NavStyle::Tabs && !search_open;
        // Paper: the search slot sits at the top of the list instead.
        let search_here = s.spec().nav != NavStyle::Rail;
        TitleBar::new().child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap_3()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Tidemail"))
                .when(tabs, |d| d.child(tabs::render(&self.state, cx)))
                .child(div().flex_1())
                .when(search_here, |d| {
                    d.child(div().w(px(360.)).child(self.search.clone()))
                })
                .when(!tabs, |d| d.child(div().flex_1()))
                .when(tabs, |d| d.child(div().w(px(12.))))
                .text_color(c.fg),
        )
    }

    /// View title and unread count above the list (Tide, Paper).
    fn list_header(&self, cx: &App) -> impl IntoElement {
        let s = self.state.read(cx);
        let c = colors(cx);
        let unread = s.unread(&s.nav.view);
        div()
            .h(px(44.))
            .flex_none()
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(px(ui::TEXT_LG + 2.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .when_some(s.spec().display_font, |d, f| d.font_family(f))
                    .child(s.nav.title.clone()),
            )
            .when(unread > 0, |d| {
                d.child(
                    div()
                        .text_size(px(ui::TEXT_SM))
                        .text_color(c.muted_fg)
                        .child(format!("{unread} unread")),
                )
            })
    }

    fn body(&self, cx: &Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let spec = s.spec();
        let c = colors(cx);
        let searching = self.search.read(cx).active;
        let row = div().flex_1().min_h_0().flex();
        if spec.reader == ReaderPlacement::Replace {
            // Command: one pane; the reader replaces the list while reading.
            return row.child(div().size_full().map(|d| match (s.reading, searching) {
                (true, _) => d.child(self.reader.clone()),
                (false, true) => d.child(self.search.read(cx).results.clone()),
                (false, false) => d.child(self.list.clone()),
            }));
        }
        let paper = spec.nav == NavStyle::Rail;
        row.when(spec.nav == NavStyle::Sidebar, |d| {
            d.child(sidebar::render(&self.state, cx))
        })
        .when(paper, |d| d.child(rail::render(&self.state, cx)))
        .child(
            div()
                .w(px(spec.list_width))
                .flex_none()
                .h_full()
                .flex()
                .flex_col()
                .relative()
                .bg(c.bg)
                .border_x_1()
                .border_color(c.border)
                .when(paper, |d| {
                    d.child(div().px_3().pt_3().child(self.search.clone()))
                })
                .when(!searching, |d| d.child(self.list_header(cx)))
                .child(div().flex_1().min_h_0().child(match searching {
                    true => self.search.read(cx).results.clone().into_any_element(),
                    false => self.list.clone().into_any_element(),
                }))
                .when_some(self.toast_content(cx), |d, t| {
                    d.child(
                        div()
                            .absolute()
                            .bottom_3()
                            .left_3()
                            .right_3()
                            .child(palette::toast::card(t, cx)),
                    )
                })
                .when(paper, |d| {
                    d.child(
                        div()
                            .px_4()
                            .py_2()
                            .child(crate::features::status::line(s, cx)),
                    )
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .bg(c.bg)
                .child(self.reader.clone()),
        )
    }

    /// Command: key hints and the status line along the bottom.
    fn hint_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let c = colors(cx);
        let hints: &[(&[&str], &str)] = if s.reading {
            &[
                (&["Esc"], "Back"),
                (&["j", "k"], "Next / previous"),
                (&["e"], "Archive"),
                (&["#"], "Trash"),
                (&["s"], "Star"),
                (&["l"], "Label"),
            ]
        } else {
            &[
                (&["j", "k"], "Move"),
                (&["Enter"], "Open"),
                (&["e"], "Archive"),
                (&["#"], "Trash"),
                (&["/"], "Search"),
                (&["g", "l"], "Labels"),
                (&["Ctrl", "K"], "Commands"),
            ]
        };
        div()
            .h(px(30.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_4()
            .bg(c.chrome)
            .border_t_1()
            .border_color(c.border)
            .map(|d| match self.toast_content(cx) {
                // Toasts take the place of the hints (never over the webview).
                Some(t) => d.child(t),
                None => d.children(
                    hints
                        .iter()
                        .map(|(keys, text)| ui::key_hint(keys, text, cx)),
                ),
            })
            .child(div().flex_1())
            .child(crate::features::status::line(s, cx))
    }
}
