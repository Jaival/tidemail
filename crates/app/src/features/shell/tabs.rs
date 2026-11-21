//! Command navigation: mailbox tabs and account dots in the title bar.
//! Labels are reached with "Go to label" (`g l`).

use gpui_kit::{prelude::FluentBuilder, *};

use super::nav;
use crate::{format, state::AppState, theme::colors, ui};

pub fn render(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let s = state.read(cx);
    let items = nav::items(s);
    let account_filter = s.nav.view.account_id;

    let tabs = items.mailboxes.into_iter().map(|item| {
        let state = state.clone();
        let nav = item.nav;
        // A mailbox tab stays active while the view is that mailbox of one account.
        let active = nav.view.label_key == s.nav.view.label_key;
        let target = nav::nav(&nav.view.label_key, account_filter, nav.title.clone());
        div()
            .id(SharedString::from(format!("tab-{}", nav.view.label_key)))
            .h(px(26.))
            .px_2p5()
            .flex()
            .items_center()
            .gap_1p5()
            .rounded(px(6.))
            .cursor_pointer()
            .when(active, |d| d.bg(c.selection).text_color(c.fg))
            .when(!active, |d| {
                d.text_color(c.muted_fg).hover(|d| d.bg(c.hover))
            })
            .child(nav.title.clone())
            .when(item.unread > 0, |d| {
                d.child(ui::count(item.unread, active, cx))
            })
            .on_click(move |_, _, cx| {
                let target = target.clone();
                state.update(cx, |s, cx| s.navigate(target, cx));
            })
    });

    let dots = s.accounts.iter().map(|a| {
        let state = state.clone();
        let id = a.id;
        let on = account_filter == Some(id);
        let name: SharedString = a
            .display_name
            .clone()
            .unwrap_or_else(|| a.email.clone())
            .into();
        div()
            .id(("acct", id as u64))
            .size(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .when(on, |d| d.border_1().border_color(c.fg))
            .hover(|d| d.bg(c.hover))
            .child(ui::dot(Some(format::parse_color(&a.color)), 10.))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx)
            })
            .on_click(move |_, _, cx| {
                state.update(cx, |s, cx| {
                    let target = nav::toggle_account(s, id);
                    s.navigate(target, cx)
                })
            })
    });

    div()
        .flex()
        .items_center()
        .gap_1()
        .children(tabs)
        .child(div().w(px(12.)))
        .children(dots)
        .child(div().w(px(12.)))
        .child(ui::key_hint(&["g", "l"], "Labels", cx))
}
