//! Search. Phase 1G-A stub: the slot in the title bar. Phase 1G-B (search
//! agent) adds the input, per-keystroke results and highlights.

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

pub fn slot(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .w_full()
        .h(px(26.))
        .px_2()
        .flex()
        .items_center()
        .rounded(theme.radius)
        .bg(theme.input)
        .text_sm()
        .text_color(theme.muted_foreground)
        .child("Search  /")
}
