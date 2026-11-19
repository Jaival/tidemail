//! Small view primitives shared by every preset: icons, icon buttons, key
//! hints, chips, avatars, section headings. Colours come from
//! [`crate::theme::colors`]; sizes are px (the window rem stays 16 px).

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{prelude::FluentBuilder, *};

use crate::theme::colors;

/// Text sizes in px.
pub const TEXT_XS: f32 = 11.0;
pub const TEXT_SM: f32 = 12.0;
pub const TEXT_LG: f32 = 15.0;
pub const TEXT_XL: f32 = 19.0;

/// Icon from the asset source, e.g. `"icons/inbox.svg"`.
pub fn icon(path: &'static str, size: f32, color: Hsla) -> Svg {
    svg()
        .path(path)
        .flex_none()
        .size(px(size))
        .text_color(color)
}

#[allow(dead_code)] // Reader toolbar (1G-B reader agent).
/// Square button with an icon and a tooltip ("Archive  e").
pub fn icon_button(
    id: impl Into<ElementId>,
    path: &'static str,
    tooltip: &'static str,
    cx: &App,
) -> Stateful<Div> {
    let c = colors(cx);
    div()
        .id(id)
        .size(px(28.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|d| d.bg(c.hover))
        .child(icon(path, 16., c.muted_fg))
        .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
}

/// Keys as small boxes, then the description: `[j] [k] Move`.
pub fn key_hint(keys: &[&'static str], text: &'static str, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .text_size(px(TEXT_XS))
        .text_color(c.muted_fg)
        .children(keys.iter().map(|k| {
            div()
                .px_1()
                .min_w(px(16.))
                .h(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(c.border)
                .text_color(c.fg)
                .child(*k)
        }))
        .child(div().ml_0p5().child(text))
}

#[allow(dead_code)] // Label chips in the reader (1G-B reader agent).
/// Rounded label: text on a tint of `color`.
pub fn chip(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .h(px(18.))
        .flex()
        .items_center()
        .rounded(px(9.))
        .bg(Hsla { a: 0.16, ..color })
        .text_color(color)
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight::MEDIUM)
        .child(text.into())
}

/// Count after a nav item or in a list header. Accent when `strong`.
pub fn count(n: i64, strong: bool, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex_none()
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if strong { c.accent } else { c.muted_fg })
        .child(n.to_string())
}

/// Stable hue for a name (FNV-1a), so a sender keeps its colour.
fn hue(name: &str) -> f32 {
    let h = name.bytes().fold(0x811c9dc5u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x01000193)
    });
    (h % 360) as f32 / 360.0
}

/// Circle with the first letter of `name` on a colour from its hash.
pub fn avatar(name: &str, size: f32) -> Div {
    let initial = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map_or_else(|| "?".to_string(), |c| c.to_uppercase().collect());
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(hsla(hue(name), 0.45, 0.42, 1.0))
        .text_color(white())
        .text_size(px(size * 0.42))
        .font_weight(FontWeight::SEMIBOLD)
        .child(initial)
}

/// Upper-case group heading in navigation and menus.
pub fn heading(text: &'static str, cx: &App) -> Div {
    div()
        .px_3()
        .pt_3()
        .pb_1()
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors(cx).muted_fg)
        .child(text)
}

/// 8 px dot, or a hidden spacer of the same size when `color` is `None`.
pub fn dot(color: Option<Hsla>, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .when_some(color, |d, c| d.bg(c))
}

#[cfg(test)]
mod tests {
    use super::hue;

    #[test]
    fn hue_is_stable_and_in_range() {
        assert_eq!(hue("Ada"), hue("Ada"));
        assert_ne!(hue("Ada"), hue("Bob"));
        assert!((0.0..1.0).contains(&hue("anything")));
    }
}
