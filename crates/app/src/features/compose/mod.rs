//! Compose a new message (Phase 3 slice 1, ADR 0004): To, Cc / Bcc behind a
//! toggle, Subject, and a plain-text body in `gpui-component`'s `Editor` set
//! up for prose (slice 0). The shell places it in the reader pane (full width
//! in Command) and sends what `ComposeEvent::Send` carries.
//!
//! Esc closes an empty message at once; with text, a second Esc within
//! `DISCARD_CONFIRM` discards it (local drafts are slice 3).

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui_kit::component::{
    input::{Editor, EditorState, Escape, Input, InputState},
    ActiveTheme,
};
use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::types::OutgoingMail;
use tidemail_gmail::compose::parse_addresses;

use crate::{
    actions::{FocusNextField, FocusPrevField, SendMail},
    state::AppState,
    theme::colors,
    ui,
};

/// Key context of the compose root. Bindings for `Input`s inside it
/// (`COMPOSE_CONTEXT > Input`) override the input's own Ctrl+Enter and Tab.
pub const CONTEXT: &str = "TidemailCompose";
/// Second Esc within this time discards a message with text.
const DISCARD_CONFIRM: Duration = Duration::from_secs(3);
/// Widest the form gets, like the reader column.
const COLUMN_MAX: f32 = 760.0;

pub enum ComposeEvent {
    Send(OutgoingMail),
    Close,
}

impl EventEmitter<ComposeEvent> for Compose {}

pub struct Compose {
    state: Entity<AppState>,
    focus: FocusHandle,
    account_id: i64,
    to: Entity<InputState>,
    cc: Entity<InputState>,
    bcc: Entity<InputState>,
    subject: Entity<InputState>,
    body: Entity<EditorState>,
    show_cc: bool,
    error: Option<SharedString>,
    /// When the first Esc asked to confirm the discard.
    confirm_discard: Option<Instant>,
    /// First paint of the form (bench: open compose to caret paint).
    painted: Rc<Cell<Option<Instant>>>,
}

impl Compose {
    /// A new message from `account_id`, filled from `mail` (undo send puts
    /// the message back). Focus goes to To, or to the body when To is set.
    pub fn new(
        state: Entity<AppState>,
        mail: OutgoingMail,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = |placeholder: &'static str, value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let value = value.to_owned();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let to = field("Recipients", &mail.to, window, cx);
        let cc = field("Cc", &mail.cc, window, cx);
        let bcc = field("Bcc", &mail.bcc, window, cx);
        let subject = field("Subject", &mail.subject, window, cx);
        // Slice 0 setup: a code editor made into a prose editor.
        let body_text = mail.body.clone();
        let body = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
                .folding(false)
                .indent_guides(false)
                .auto_close(false)
                .smart_indent(false)
                .placeholder("Write your message")
                .default_value(body_text)
        });
        if mail.to.is_empty() {
            to.update(cx, |i, cx| i.focus(window, cx));
        } else {
            body.update(cx, |b, cx| b.focus(window, cx));
        }
        Self {
            state,
            focus: cx.focus_handle(),
            account_id: mail.account_id,
            show_cc: !mail.cc.is_empty() || !mail.bcc.is_empty(),
            to,
            cc,
            bcc,
            subject,
            body,
            error: None,
            confirm_discard: None,
            painted: Rc::default(),
        }
    }

    /// When the form (with the caret in it) was first painted.
    pub fn painted(&self) -> Option<Instant> {
        self.painted.get()
    }

    /// Compose has keyboard focus (the shell drops its single-key bindings).
    pub fn focused(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.to.update(cx, |i, cx| i.focus(window, cx));
    }

    fn mail(&self, cx: &App) -> OutgoingMail {
        let v = |i: &Entity<InputState>| i.read(cx).value().trim().to_string();
        OutgoingMail {
            account_id: self.account_id,
            to: v(&self.to),
            cc: v(&self.cc),
            bcc: v(&self.bcc),
            subject: v(&self.subject),
            body: self.body.read(cx).value().to_string(),
        }
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        let mail = self.mail(cx);
        let mut count = 0;
        for list in [&mail.to, &mail.cc, &mail.bcc] {
            match parse_addresses(list) {
                Ok(v) => count += v.len(),
                Err(e) => return self.fail(e, cx),
            }
        }
        if count == 0 {
            return self.fail("Add at least one recipient".into(), cx);
        }
        cx.emit(ComposeEvent::Send(mail));
    }

    fn fail(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error.into());
        cx.notify();
    }

    /// Esc: close when empty; with text, ask once, then discard.
    fn escape(&mut self, cx: &mut Context<Self>) {
        let m = self.mail(cx);
        let empty = m.to.is_empty()
            && m.cc.is_empty()
            && m.bcc.is_empty()
            && m.subject.is_empty()
            && m.body.trim().is_empty();
        let confirmed = self
            .confirm_discard
            .is_some_and(|t| t.elapsed() < DISCARD_CONFIRM);
        if empty || confirmed {
            cx.emit(ComposeEvent::Close);
        } else {
            self.confirm_discard = Some(Instant::now());
            cx.notify();
            // Hide the hint when the confirm window ends.
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(DISCARD_CONFIRM).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
    }

    /// Tab order: To, (Cc, Bcc), Subject, body.
    fn move_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut order: Vec<FocusHandle> = vec![self.to.read(cx).focus_handle(cx)];
        if self.show_cc {
            order.push(self.cc.read(cx).focus_handle(cx));
            order.push(self.bcc.read(cx).focus_handle(cx));
        }
        order.push(self.subject.read(cx).focus_handle(cx));
        order.push(self.body.read(cx).focus_handle(cx));
        let at = order.iter().position(|f| f.is_focused(window)).unwrap_or(0);
        let n = order.len();
        let next = if forward { (at + 1) % n } else { (at + n - 1) % n };
        order[next].focus(window, cx);
    }

    fn cycle_account(&mut self, cx: &mut Context<Self>) {
        let accounts = &self.state.read(cx).accounts;
        if let Some(i) = accounts.iter().position(|a| a.id == self.account_id) {
            self.account_id = accounts[(i + 1) % accounts.len()].id;
            cx.notify();
        }
    }

    fn row(&self, label: &'static str, field: AnyElement, cx: &App) -> Div {
        let c = colors(cx);
        div()
            .flex_none()
            .h(px(36.))
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(c.border)
            .child(
                div()
                    .w(px(56.))
                    .flex_none()
                    .text_size(px(ui::TEXT_SM))
                    .text_color(c.muted_fg)
                    .child(label),
            )
            .child(div().flex_1().min_w_0().child(field))
    }
}

impl Render for Compose {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let font = cx.theme().font_family.clone();
        let input = |i: &Entity<InputState>| Input::new(i).appearance(false).into_any_element();
        let s = self.state.read(cx);
        let from = s
            .account(self.account_id)
            .map(|a| a.email.clone())
            .unwrap_or_default();
        let many = s.accounts.len() > 1;
        let confirming = self
            .confirm_discard
            .is_some_and(|t| t.elapsed() < DISCARD_CONFIRM);
        let painted = self.painted.clone();
        div()
            .id("compose")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .justify_center()
            .on_action(cx.listener(|this, _: &SendMail, _, cx| this.send(cx)))
            // Inputs propagate Escape when they have nothing to dismiss.
            .on_action(cx.listener(|this, _: &Escape, _, cx| this.escape(cx)))
            .on_action(cx.listener(|this, _: &FocusNextField, window, cx| {
                this.move_focus(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusPrevField, window, cx| {
                this.move_focus(false, window, cx)
            }))
            .child(
                div()
                    .size_full()
                    .max_w(px(COLUMN_MAX + 32.))
                    .px_4()
                    .pb_4()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(48.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .text_size(px(ui::TEXT_LG + 2.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("New message"),
                    )
                    .child(self.row(
                        "From",
                        div()
                            .id("compose-from")
                            .text_size(px(ui::TEXT_SM + 1.))
                            .when(many, |d| {
                                d.cursor_pointer()
                                    .hover(|d| d.text_color(c.accent))
                                    .on_click(cx.listener(|this, _, _, cx| this.cycle_account(cx)))
                            })
                            .child(from)
                            .into_any_element(),
                        cx,
                    ))
                    .child(
                        self.row(
                            "To",
                            div()
                                .flex()
                                .items_center()
                                .child(div().flex_1().min_w_0().child(input(&self.to)))
                                .when(!self.show_cc, |d| {
                                    d.child(
                                        div()
                                            .id("compose-cc-toggle")
                                            .flex_none()
                                            .px_1()
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .text_size(px(ui::TEXT_SM))
                                            .text_color(c.muted_fg)
                                            .hover(|d| d.bg(c.hover))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.show_cc = true;
                                                this.cc.update(cx, |i, cx| i.focus(window, cx));
                                                cx.notify();
                                            }))
                                            .child("Cc Bcc"),
                                    )
                                })
                                .into_any_element(),
                            cx,
                        ),
                    )
                    .when(self.show_cc, |d| {
                        d.child(self.row("Cc", input(&self.cc), cx))
                            .child(self.row("Bcc", input(&self.bcc), cx))
                    })
                    .child(self.row("Subject", input(&self.subject), cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .pt_3()
                            .child(
                                Editor::new(&self.body)
                                    .font_family(font)
                                    .text_size(px(ui::TEXT_LG))
                                    .h_full(),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .pt_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                ui::button("compose-send", "Send", true, c)
                                    .on_click(cx.listener(|this, _, _, cx| this.send(cx))),
                            )
                            .child(ui::key_hint(&["Ctrl", "Enter"], "Send", cx))
                            .child(ui::key_hint(&["Esc"], "Discard", cx))
                            .child(div().flex_1())
                            .map(|d| match (&self.error, confirming) {
                                (_, true) => d.child(
                                    div()
                                        .text_size(px(ui::TEXT_SM))
                                        .text_color(c.warning)
                                        .child("Press Esc again to discard this message"),
                                ),
                                (Some(e), false) => d.child(
                                    div()
                                        .text_size(px(ui::TEXT_SM))
                                        .text_color(c.danger)
                                        .child(e.clone()),
                                ),
                                (None, false) => d,
                            }),
                    ),
            )
            // Painted after the form (and its caret).
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, _, _| {
                        if painted.get().is_none() {
                            painted.set(Some(Instant::now()));
                        }
                    },
                )
                .size_0(),
            )
    }
}
