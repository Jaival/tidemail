//! Tide navigation: unified mailboxes, accounts (with their mailboxes when
//! current), user labels, status line.

use gpui_kit::{prelude::FluentBuilder, *};

use super::nav::{self, Item};
use crate::{state::AppState, theme::colors, ui};

fn row(item: Item, sub: bool, state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let state = state.clone();
    let view = &item.nav.view;
    let id = SharedString::from(format!(
        "nav-{}-{}",
        view.label_key,
        view.account_id.unwrap_or(0)
    ));
    let lead = match (item.icon, item.color) {
        (Some(path), _) => {
            ui::icon(path, 15., if item.active { c.accent } else { c.muted_fg }).into_any_element()
        }
        (None, color) => ui::dot(color, 8.).into_any_element(),
    };
    let nav = item.nav;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .h(px(28.))
        .px_2()
        .when(sub, |d| d.pl(px(26.)))
        .rounded(px(6.))
        .cursor_pointer()
        .when(item.active, |d| {
            d.bg(c.selection).font_weight(FontWeight::MEDIUM)
        })
        .when(!item.active, |d| d.hover(|d| d.bg(c.hover)))
        .child(div().w(px(16.)).flex().justify_center().child(lead))
        .child(div().flex_1().min_w_0().truncate().child(nav.title.clone()))
        .when(item.unread > 0, |d| {
            d.child(ui::count(item.unread, item.active, cx))
        })
        .on_click(move |_, _, cx| {
            let nav = nav.clone();
            state.update(cx, |s, cx| s.navigate(nav, cx));
        })
}

pub fn render(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let s = state.read(cx);
    let items = nav::items(s);

    let mut list = div().flex().flex_col().gap_px().px_2().pt_2();
    for item in items.mailboxes {
        list = list.child(row(item, false, state, cx));
    }
    if !items.accounts.is_empty() {
        list = list.child(ui::heading("ACCOUNTS", cx));
    }
    for a in items.accounts {
        list = list.child(row(a.item, false, state, cx));
        for item in a.sub {
            list = list.child(row(item, true, state, cx));
        }
    }
    if !items.labels.is_empty() {
        list = list.child(ui::heading("LABELS", cx));
    }
    for item in items.labels {
        list = list.child(row(item, false, state, cx));
    }

    div()
        .w(px(s.spec().nav_width))
        .flex_none()
        .h_full()
        .flex()
        .flex_col()
        .bg(c.chrome)
        .child(
            div()
                .id("sidebar-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(list),
        )
        .child(
            div()
                .px_4()
                .py_3()
                .child(crate::features::status::line(state, cx)),
        )
}
