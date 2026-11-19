//! Reader pane. Phase 1G-A stub: native header and message list of the opened
//! thread. Phase 1G-B (reader agent) adds the WebView2 body host.

use gpui_kit::component::ActiveTheme;
use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::{read, types::ThreadDetail};

use crate::{
    format,
    state::{AppState, StateEvent},
};

pub struct Reader {
    state: Entity<AppState>,
    thread: Option<ThreadDetail>,
    loading: Option<Task<()>>,
    _subscription: Subscription,
}

impl Reader {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let sub = cx.subscribe(&state, |this, _, event, cx| match event {
            StateEvent::SelectionChanged => this.load(cx),
            StateEvent::DataChanged { thread_ids, .. }
                if this
                    .thread
                    .as_ref()
                    .is_some_and(|t| thread_ids.contains(&t.thread.id)) =>
            {
                this.load(cx)
            }
            _ => {}
        });
        Self {
            state,
            thread: None,
            loading: None,
            _subscription: sub,
        }
    }

    /// Thread whose content is on screen (bench: open-to-paint).
    pub fn shown(&self) -> Option<i64> {
        self.thread.as_ref().map(|t| t.thread.id)
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.state.read(cx).selected else {
            self.thread = None;
            self.loading = None;
            cx.notify();
            return;
        };
        let task = self
            .state
            .read(cx)
            .db
            .read(cx, move |c| read::get_thread(c, id));
        self.loading = Some(cx.spawn(async move |this, cx| {
            let detail = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = None;
                this.thread = detail.ok().flatten();
                cx.notify();
            });
        }));
    }
}

impl Render for Reader {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let colors = crate::theme::colors(cx);
        let Some(detail) = &self.thread else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Select a thread")
                .into_any_element();
        };
        let subject: SharedString = if detail.thread.subject.is_empty() {
            "(no subject)".into()
        } else {
            detail.thread.subject.clone().into()
        };
        div()
            .id("reader")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(
                div()
                    .text_size(px(crate::ui::TEXT_XL))
                    .font_weight(FontWeight::SEMIBOLD)
                    .when_some(self.state.read(cx).spec().display_font, |d, f| {
                        d.font_family(f)
                    })
                    .child(subject),
            )
            .children(detail.messages.iter().map(|m| {
                let from = if m.from_name.is_empty() {
                    &m.from_email
                } else {
                    &m.from_name
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded(theme.radius)
                    .bg(colors.paper)
                    .text_color(colors.paper_fg)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(from.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format::list_date(m.internal_date)),
                            ),
                    )
                    .child(div().text_sm().child(m.snippet.clone()))
            }))
            .into_any_element()
    }
}
