//! Window content: title bar with the search slot, sidebar, thread list, reader.
//! Owns keyboard focus and routes actions.

mod nav;
mod rail;
mod sidebar;
mod tabs;

use gpui_kit::component::{ActiveTheme, TitleBar};
use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::types::{ActionKind, ThreadPage};

use crate::{
    actions::*,
    features::{list::ThreadList, reader::Reader, search},
    preset::{NavStyle, ReaderPlacement},
    state::{AppState, Nav, StateEvent},
    theme::colors,
    ui,
};

pub struct Shell {
    pub state: Entity<AppState>,
    pub list: Entity<ThreadList>,
    pub reader: Entity<Reader>,
    focus: FocusHandle,
    _subscription: Subscription,
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
        let sub = cx.subscribe(&state, |_, _, event, cx| {
            if matches!(
                event,
                StateEvent::NavChanged
                    | StateEvent::AccountsChanged
                    | StateEvent::StatusChanged
                    | StateEvent::ReadingChanged
                    | StateEvent::SettingsChanged
            ) {
                cx.notify();
            }
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            state,
            list,
            reader,
            focus,
            _subscription: sub,
        }
    }

    /// Apply `kind` to the opened thread, with an undo toast.
    fn act(
        &mut self,
        kind: impl FnOnce(&tidemail_db::types::ThreadRow) -> (ActionKind, &'static str),
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.list.read(cx).current(cx) else {
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
        context.add(CONTEXT);
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
                this.list.update(cx, |l, cx| l.select_next(cx))
            }))
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| {
                this.list.update(cx, |l, cx| l.select_prev(cx))
            }))
            .on_action(cx.listener(|this, _: &OpenThread, _, cx| {
                this.list.update(cx, |l, cx| l.open_current(cx))
            }))
            .on_action(cx.listener(|this, _: &CloseReader, _, cx| {
                this.state.update(cx, |s, cx| s.close_reader(cx))
            }))
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
            .child(self.title_bar(cx))
            .child(self.body(cx))
            .when(spec.hint_bar, |d| d.child(self.hint_bar(cx)))
    }
}

impl Shell {
    fn title_bar(&self, cx: &App) -> impl IntoElement {
        let s = self.state.read(cx);
        let c = colors(cx);
        let tabs = s.spec().nav == NavStyle::Tabs;
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
                    d.child(div().w(px(360.)).child(search::slot(cx)))
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

    fn body(&self, cx: &App) -> impl IntoElement {
        let s = self.state.read(cx);
        let spec = s.spec();
        let c = colors(cx);
        let row = div().flex_1().min_h_0().flex();
        if spec.reader == ReaderPlacement::Replace {
            // Command: one pane; the reader replaces the list while reading.
            return row.child(div().size_full().map(|d| match s.reading {
                true => d.child(self.reader.clone()),
                false => d.child(self.list.clone()),
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
                .bg(c.bg)
                .border_x_1()
                .border_color(c.border)
                .when(paper, |d| {
                    d.child(div().px_3().pt_3().child(search::slot(cx)))
                })
                .child(self.list_header(cx))
                .child(div().flex_1().min_h_0().child(self.list.clone()))
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
    fn hint_bar(&self, cx: &App) -> impl IntoElement {
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
            .children(
                hints
                    .iter()
                    .map(|(keys, text)| ui::key_hint(keys, text, cx)),
            )
            .child(div().flex_1())
            .child(crate::features::status::line(s, cx))
    }
}
