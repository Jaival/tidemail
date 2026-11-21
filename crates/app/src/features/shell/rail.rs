//! Paper navigation: a 44 px icon rail with the mailboxes and account dots.
//! Labels are reached with "Go to label" (`g l`).

use gpui_kit::{component::tooltip::Tooltip, prelude::FluentBuilder, *};

use super::nav;
use crate::{format, state::AppState, theme::colors, ui};

pub fn render(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let s = state.read(cx);
    let items = nav::items(s);
    let account_filter = s.nav.view.account_id;

    let button = |id: SharedString, tip: SharedString, active: bool| {
        div()
            .id(id)
            .relative()
            .size(px(32.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .when(active, |d| d.bg(c.selection))
            .when(!active, |d| d.hover(|d| d.bg(c.hover)))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
    };

    let mailboxes = items.mailboxes.into_iter().map(|item| {
        let state = state.clone();
        let key = item.nav.view.label_key.clone();
        let active = key == s.nav.view.label_key;
        let target = nav::nav(&key, account_filter, item.nav.title.clone());
        let tip: SharedString = match item.unread {
            0 => item.nav.title.clone(),
            n => format!("{}  {n}", item.nav.title).into(),
        };
        button(format!("rail-{key}").into(), tip, active)
            .child(ui::icon(
                item.icon.unwrap_or("icons/inbox.svg"),
                17.,
                if active { c.accent } else { c.muted_fg },
            ))
            .when(item.unread > 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(6.))
                        .right(px(6.))
                        .child(ui::dot(Some(c.accent), 6.)),
                )
            })
            .on_click(move |_, _, cx| {
                let target = target.clone();
                state.update(cx, |s, cx| s.navigate(target, cx));
            })
    });

    let accounts = s.accounts.iter().map(|a| {
        let state = state.clone();
        let id = a.id;
        let on = account_filter == Some(id);
        let name: SharedString = a
            .display_name
            .clone()
            .unwrap_or_else(|| a.email.clone())
            .into();
        button(format!("rail-acct-{id}").into(), name, on)
            .child(ui::dot(Some(format::parse_color(&a.color)), 10.))
            .on_click(move |_, _, cx| {
                state.update(cx, |s, cx| {
                    let target = nav::toggle_account(s, id);
                    s.navigate(target, cx)
                })
            })
    });

    div()
        .w(px(s.spec().nav_width))
        .flex_none()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .py_2()
        .bg(c.chrome)
        .children(mailboxes)
        .child(div().my_1().w(px(20.)).h_px().bg(c.border))
        .children(accounts)
}
