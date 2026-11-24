//! Toast content: an action result with "Undo", or an error. The shell owns
//! the current toast and places it (ADR 0002): bottom of the list pane in
//! Tide and Paper, the hint bar in Command. Never over the webview, so no
//! `Overlay::Toast` is pushed.

use std::time::Duration;

use gpui_kit::{prelude::FluentBuilder, *};

use crate::{theme::colors, ui};

/// How long a toast stays (also the undo window of the engine).
pub const LIFETIME: Duration = Duration::from_secs(5);
/// "Undo send" stays for the send queue's undo window (ADR 0004).
pub const SEND_LIFETIME: Duration = Duration::from_secs(10);
/// Notices and errors have no undo window; they stay long enough to read.
pub const NOTICE_LIFETIME: Duration = Duration::from_secs(12);

pub struct Toast {
    pub text: SharedString,
    /// What "Undo" reverts; `None` for errors and notices.
    pub undo: Option<Undo>,
    pub error: bool,
}

#[derive(Clone)]
pub enum Undo {
    /// Action queue rows (`AppState::undo`).
    Actions(Vec<i64>),
    /// An outbox row (`AppState::take_back_send`).
    Send(i64),
}

impl Toast {
    pub fn lifetime(&self) -> Duration {
        match self.undo {
            Some(Undo::Actions(_)) => LIFETIME,
            Some(Undo::Send(_)) => SEND_LIFETIME,
            None => NOTICE_LIFETIME,
        }
    }
}

/// Text, then an "Undo" button with its key. `on_undo` runs on click.
pub fn content(
    toast: &Toast,
    on_undo: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let c = colors(cx);
    let error = toast.error;
    div()
        .flex()
        .items_center()
        .gap_3()
        .text_size(px(ui::TEXT_SM))
        .child(
            div()
                .text_color(if error { c.danger } else { c.fg })
                .child(toast.text.clone()),
        )
        .when(toast.undo.is_some(), |d| {
            d.child(
                div()
                    .id("toast-undo")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(|d| d.bg(c.hover))
                    .text_color(c.accent)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Undo")
                    .child(ui::key_hint(&["Ctrl", "Z"], "", cx))
                    .on_click(on_undo),
            )
        })
}

/// Content on a floating card (list pane).
pub fn card(content: Div, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .px_3()
        .py_2()
        .rounded(px(8.))
        .border_1()
        .border_color(c.border)
        .bg(c.chrome)
        .shadow_md()
        .child(content)
}
