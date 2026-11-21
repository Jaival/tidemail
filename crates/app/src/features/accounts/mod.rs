//! The Accounts and sync overlay (`Overlay::Accounts`). A new account keeps
//! the last 6 months of mail on this PC (`sync::Config::sync_window`). Per
//! account it shows how far back the mail goes, the backfill progress, and
//! buttons to download older mail, with what that costs above them.

use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::types::{Account, AccountSync};

use crate::{
    actions::OpenThread,
    format,
    state::{AppState, Overlay, StateEvent},
    theme::{colors, Colors},
    ui,
};

const DAY_MS: i64 = 86_400_000;

/// "Download older mail" choices: label and days back (`None`: all mail).
const CHOICES: [(&str, Option<i64>); 3] = [
    ("1 year", Some(365)),
    ("2 years", Some(730)),
    ("All mail", None),
];

/// Shown above the buttons: what downloading older mail costs.
const DISCLAIMER: [&str; 5] = [
    "A big mailbox can take many hours. Gmail limits how fast apps may download mail.",
    "The download runs while Tidemail is open or in the tray, and needs internet.",
    "Older mail uses more disk space on this PC.",
    "The text of mail older than 6 months downloads when you first open it.",
    "You cannot make the range smaller again. Sign out (Ctrl+K) removes an account's mail from this PC.",
];

pub struct AccountsPanel {
    state: Entity<AppState>,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl AccountsPanel {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sub = cx.subscribe(&state, |_, _, event, cx| {
            if matches!(
                event,
                StateEvent::StatusChanged
                    | StateEvent::AccountsChanged
                    | StateEvent::SettingsChanged
            ) {
                cx.notify();
            }
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            state,
            focus,
            _subscription: sub,
        }
    }

    fn close(&self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |s, cx| s.close_overlay(&Overlay::Accounts, cx));
    }

    /// Widen the account's window; the engine reports the new status.
    fn extend(&self, account_id: i64, from: Option<i64>, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let engine = state.read(cx).engine.clone();
        let task = cx.background_spawn(async move { engine.extend_sync(account_id, from) });
        cx.spawn(async move |_, cx| {
            if let Err(e) = task.await {
                state.update(cx, |_, cx| {
                    cx.emit(StateEvent::ActionFailed {
                        text: format!("Could not start the download. {}", format::user_error(&e))
                            .into(),
                    })
                });
            }
        })
        .detach();
    }

    fn card(
        &self,
        account: &Account,
        sync: Option<&AccountSync>,
        now: i64,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> Div {
        let from = sync.and_then(|s| s.sync_from);
        let held = match (sync, from) {
            (None, _) => "Not synced yet".to_owned(),
            (Some(_), None) => "All mail is on this PC".to_owned(),
            (Some(_), Some(f)) => format!("Mail since {} is on this PC", format::long_date(f)),
        };
        let progress = sync.and_then(|s| s.backfill).map(|b| {
            let what = if b.older {
                "Downloading older mail"
            } else {
                "First sync"
            };
            format!("{what} {} %", b.percent)
        });
        // Only choices that reach at least a week further back.
        let choices: Vec<(&str, Option<i64>)> = CHOICES
            .into_iter()
            .map(|(label, days)| (label, days.map(|d| now - d * DAY_MS)))
            .filter(|(_, to)| match (sync.is_some(), from, to) {
                (false, ..) | (_, None, _) => false,
                (_, Some(_), None) => true,
                (_, Some(f), Some(t)) => *t < f - 7 * DAY_MS,
            })
            .collect();
        let id = account.id;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(8.))
            .border_1()
            .border_color(c.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(ui::dot(Some(format::parse_color(&account.color)), 8.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(account.email.clone()),
                    ),
            )
            .child(
                div()
                    .text_size(px(ui::TEXT_SM))
                    .text_color(c.muted_fg)
                    .child(match &progress {
                        Some(p) => format!("{held} · {p}"),
                        None => held,
                    }),
            )
            .when(!choices.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(ui::TEXT_SM))
                                .child("Download older mail:"),
                        )
                        .children(choices.into_iter().map(|(label, to)| {
                            ui::button(
                                SharedString::from(format!("accounts-{id}-{label}")),
                                label,
                                false,
                                c,
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.extend(id, to, cx)))
                        })),
                )
            })
    }
}

impl Render for AccountsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = *colors(cx);
        let now = tidemail_db::now_ms();
        let (accounts, status) = {
            let s = self.state.read(cx);
            (s.accounts.clone(), s.status.accounts.clone())
        };
        let cards: Vec<Div> = accounts
            .iter()
            .map(|a| {
                let sync = status.iter().find(|s| s.account_id == a.id);
                self.card(a, sync, now, &c, cx)
            })
            .collect();
        let disclaimer = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(12.))
            .rounded(px(8.))
            .bg(c.chrome)
            .text_size(px(ui::TEXT_SM))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Before you download older mail"),
            )
            .children(DISCLAIMER.map(|line| {
                div()
                    .flex()
                    .gap(px(6.))
                    .text_color(c.muted_fg)
                    .child("•")
                    .child(div().flex_1().min_w_0().child(line))
            }));
        let max_h = window.viewport_size().height - px(72.);
        let panel = div()
            .id("accounts")
            .key_context("TidemailAccounts")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OpenThread, _, cx| this.close(cx)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .w(px(560.))
            .max_h(max_h)
            .flex()
            .flex_col()
            .rounded(px(10.))
            .border_1()
            .border_color(c.border)
            .bg(c.bg)
            .text_color(c.fg)
            .shadow_lg()
            .child(
                div()
                    .p(px(18.))
                    .pb(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(ui::TEXT_LG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Accounts and sync"),
                    )
                    .child(
                        div()
                            .text_size(px(ui::TEXT_SM))
                            .text_color(c.muted_fg)
                            .child("Tidemail keeps the last 6 months of mail on this PC. You can download older mail per account."),
                    ),
            )
            .child(
                div()
                    .id("accounts-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(18.))
                    .pb(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(cards.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(ui::TEXT_SM))
                                .text_color(c.muted_fg)
                                .child("No accounts yet."),
                        )
                    })
                    .children(cards)
                    .child(disclaimer),
            )
            .child(
                div()
                    .p(px(14.))
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .border_t_1()
                    .border_color(c.border)
                    .child(ui::button("accounts-add", "Add account…", false, &c).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.close(cx);
                            this.state.update(cx, |s, cx| s.add_account(cx));
                        }),
                    ))
                    .child(div().flex_1())
                    .child(
                        ui::button("accounts-done", "Done", true, &c)
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    ),
            );
        deferred(
            div()
                .id("accounts-layer")
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_end()
                .items_start()
                .pt(px(52.))
                .pr(px(16.))
                .child(panel),
        )
    }
}
