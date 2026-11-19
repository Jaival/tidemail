//! Thread rows, one renderer per `RowStyle`. No borders: the selected row is a
//! tinted, rounded fill; unread is an accent dot and bold text; in unified
//! views a 2 px rail on the left shows the account colour.

use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::types::ThreadRow;

use crate::{
    features::search::Snippet,
    format,
    preset::RowStyle,
    theme::{colors, Colors},
    ui,
};

pub struct Row<'a> {
    pub thread: &'a ThreadRow,
    pub selected: bool,
    /// Account colour, in unified views only.
    pub account: Option<Hsla>,
    pub display_font: Option<&'static str>,
    /// Search results: the matched snippet replaces the thread snippet and
    /// its marked runs are highlighted.
    pub snippet: Option<&'a Snippet>,
}

impl Row<'_> {
    fn unread(&self) -> bool {
        self.thread.unread_count > 0
    }

    fn sender(&self) -> SharedString {
        match self.thread.participants.as_str() {
            "" => "Unknown sender".into(),
            p => p.to_string().into(),
        }
    }

    fn subject(&self) -> SharedString {
        match self.thread.subject.as_str() {
            "" => "(no subject)".into(),
            s => s.to_string().into(),
        }
    }

    fn weight(&self) -> FontWeight {
        if self.unread() {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        }
    }
}

/// Unread dot column (keeps text aligned when read).
fn unread_dot(row: &Row, c: &Colors) -> Div {
    ui::dot(row.unread().then_some(c.accent), 7.)
}

fn date(row: &Row, c: &Colors) -> Div {
    div()
        .flex_none()
        .text_size(px(ui::TEXT_XS))
        .text_color(if row.unread() { c.fg } else { c.muted_fg })
        .child(format::list_date(row.thread.last_date))
}

fn star(row: &Row, c: &Colors) -> Option<Svg> {
    row.thread
        .is_starred
        .then(|| ui::icon("icons/star-fill.svg", 13., c.warning))
}

fn count(row: &Row, c: &Colors) -> Option<Div> {
    (row.thread.message_count > 1).then(|| {
        div()
            .flex_none()
            .text_size(px(ui::TEXT_XS))
            .text_color(c.muted_fg)
            .child(row.thread.message_count.to_string())
    })
}

/// The snippet after `prefix`; marked runs (search results) in the accent colour.
fn snippet_text(row: &Row, prefix: &str, c: &Colors) -> StyledText {
    let Some(s) = row.snippet else {
        return StyledText::new(format!("{prefix}{}", row.thread.snippet));
    };
    let style = HighlightStyle {
        color: Some(c.accent),
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let n = prefix.len();
    StyledText::new(format!("{prefix}{}", s.text))
        .with_highlights(s.marks.iter().map(|m| (m.start + n..m.end + n, style)))
}

/// Tide: sender and date, then subject — snippet. 56 px.
fn two_line(row: &Row, height: f32, c: &Colors) -> Div {
    let line1 = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(row.weight())
                .child(row.sender()),
        )
        .children(count(row, c))
        .children(star(row, c))
        .child(date(row, c));
    let line2 = div()
        .flex()
        .min_w_0()
        .text_size(px(ui::TEXT_SM))
        .child(
            div()
                .flex_none()
                .max_w(relative(0.7))
                .truncate()
                .when(row.unread(), |d| d.font_weight(FontWeight::MEDIUM))
                .child(row.subject()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(c.muted_fg)
                .child(snippet_text(row, " — ", c)),
        );
    content(row, height, c, |d| {
        d.child(unread_dot(row, c)).child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(line1)
                .child(line2),
        )
    })
}

/// Command: one dense line. 32 px.
fn one_line(row: &Row, height: f32, c: &Colors) -> Div {
    content(row, height, c, |d| {
        d.child(unread_dot(row, c))
            .child(
                div()
                    .w(px(168.))
                    .flex_none()
                    .truncate()
                    .font_weight(row.weight())
                    .child(row.sender()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .child(
                        div()
                            .flex_none()
                            .max_w(relative(0.6))
                            .truncate()
                            .font_weight(row.weight())
                            .child(row.subject()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(c.muted_fg)
                            .child(snippet_text(row, "  ", c)),
                    ),
            )
            .children(count(row, c))
            .children(star(row, c))
            .child(date(row, c))
    })
}

/// Paper: avatar; sender and date, subject in the display font, snippet. 76 px.
fn three_line(row: &Row, height: f32, c: &Colors) -> Div {
    let sender = row.sender();
    content(row, height, c, |d| {
        d.items_start()
            .pt(px(10.))
            .child(ui::avatar(&sender, 32.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(ui::TEXT_SM))
                                    .font_weight(row.weight())
                                    .when(!row.unread(), |d| d.text_color(c.muted_fg))
                                    .child(sender),
                            )
                            .when(row.unread(), |d| d.child(unread_dot(row, c)))
                            .children(star(row, c))
                            .child(date(row, c)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.5))
                            .font_weight(row.weight())
                            .when_some(row.display_font, |d, f| d.font_family(f))
                            .child(row.subject()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(ui::TEXT_SM))
                            .text_color(c.muted_fg)
                            .child(snippet_text(row, "", c)),
                    ),
            )
    })
}

/// Row frame (fixed height, inset rounded fill, account rail) with `build`
/// applied to the fill.
fn content(row: &Row, height: f32, c: &Colors, build: impl FnOnce(Div) -> Div) -> Div {
    let inner = div()
        .relative()
        .size_full()
        .flex()
        .items_center()
        .gap_2()
        .pl(px(10.))
        .pr(px(10.))
        .rounded(px(6.))
        .when(row.selected, |d| d.bg(c.selection))
        .when(!row.selected, |d| d.hover(|d| d.bg(c.hover)))
        .when_some(row.account, |d, color| {
            d.child(
                div()
                    .absolute()
                    .left(px(2.))
                    .top(px(6.))
                    .bottom(px(6.))
                    .w(px(2.))
                    .rounded(px(1.))
                    .bg(color),
            )
        });
    div()
        .h(px(height))
        .w_full()
        .px(px(6.))
        .py(px(1.))
        .child(build(inner))
}

pub fn render(style: RowStyle, height: f32, row: &Row, cx: &App) -> Div {
    let c = colors(cx);
    match style {
        RowStyle::TwoLine => two_line(row, height, c),
        RowStyle::OneLine => one_line(row, height, c),
        RowStyle::ThreeLine => three_line(row, height, c),
    }
}
