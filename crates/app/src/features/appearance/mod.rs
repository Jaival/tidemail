//! The Appearance overlay (`Overlay::Appearance`): Light / Dark / System, one
//! colour choice per mode (preset colours, a built-in theme or the custom
//! accent), the accent itself (12 swatches, a hue slider, two switches, a
//! contrast readout), and on the Fonts tab the fonts and their switches.
//!
//! Every change applies at once and shows the mode being edited; nothing is
//! saved until Done. Esc or Cancel puts back what was there when it opened.
//! No dimmed backdrop: the panel sits at the right so the app shows each
//! change.

use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{prelude::FluentBuilder, *};

use crate::{
    actions::OpenThread,
    fonts::{FontInfo, Fonts, CODE_FONTS, UI_FONTS},
    settings::{ColorChoice, Settings, ThemePref},
    state::{AppState, Overlay},
    theme::{
        self, colors,
        derive::{self, Accent, Chroma, SWATCHES},
        Colors, Palette, ThemeId,
    },
    ui,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Colours,
    Fonts,
}

pub struct Appearance {
    state: Entity<AppState>,
    /// Settings when it opened; Cancel puts them back.
    original: Settings,
    draft: Settings,
    /// Which mode's colours are edited (and shown meanwhile).
    editing_dark: bool,
    tab: Tab,
    /// The overlay this view belongs to.
    overlay: Overlay,
    hue: Entity<SliderState>,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl Appearance {
    pub fn new(
        state: Entity<AppState>,
        fonts_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let original = state.read(cx).settings.clone();
        let hue = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(359.)
                .step(1.)
                .default_value(f32::from(original.accent.hue))
        });
        let sub = cx.subscribe_in(&hue, window, |this, _, ev: &SliderEvent, window, cx| {
            if let SliderEvent::Change(v) = ev {
                let hue = (v.start().round() as u16).min(359);
                this.edit_accent(|a| a.hue = hue, window, cx);
            }
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            editing_dark: theme::is_dark(original.theme, window.appearance()),
            state,
            draft: original.clone(),
            original,
            tab: if fonts_tab { Tab::Fonts } else { Tab::Colours },
            overlay: Overlay::Appearance { fonts: fonts_tab },
            hue,
            focus,
            _subscription: sub,
        }
    }

    pub fn overlay(&self) -> Overlay {
        self.overlay.clone()
    }

    /// Show the draft in the edited mode; the state holds it unsaved.
    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        theme::apply_mode(&self.draft, self.editing_dark, window, cx);
        let draft = self.draft.clone();
        self.state
            .update(cx, |s, cx| s.set_settings(draft, false, cx));
        window.refresh();
        cx.notify();
    }

    fn edit(&mut self, f: impl FnOnce(&mut Settings), window: &mut Window, cx: &mut Context<Self>) {
        f(&mut self.draft);
        self.preview(window, cx);
    }

    fn set_mode(&mut self, pref: ThemePref, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_dark = theme::is_dark(pref, window.appearance());
        self.edit(|s| s.theme = pref, window, cx);
    }

    fn edit_mode(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_dark = dark;
        self.preview(window, cx);
    }

    fn choice(&self) -> ColorChoice {
        if self.editing_dark {
            self.draft.colors_dark
        } else {
            self.draft.colors_light
        }
    }

    fn pick(&mut self, choice: ColorChoice, window: &mut Window, cx: &mut Context<Self>) {
        let dark = self.editing_dark;
        self.edit(
            |s| match dark {
                true => s.colors_dark = choice,
                false => s.colors_light = choice,
            },
            window,
            cx,
        );
    }

    /// Change the accent; the edited mode switches to it too.
    fn edit_accent(&mut self, f: impl FnOnce(&mut Accent), window: &mut Window, cx: &mut Context<Self>) {
        f(&mut self.draft.accent);
        self.pick(ColorChoice::Accent, window, cx);
    }

    fn close(&self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |s, cx| s.close_overlay(&self.overlay, cx));
    }

    /// Put back the settings from when it opened.
    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        theme::apply(&self.original, window, cx);
        let original = self.original.clone();
        self.state
            .update(cx, |s, cx| s.set_settings(original, false, cx));
        window.refresh();
        self.close(cx);
    }

    /// Keep and save the draft; show the saved mode.
    fn done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        theme::apply(&self.draft, window, cx);
        let draft = self.draft.clone();
        self.state.update(cx, |s, cx| s.set_settings(draft, true, cx));
        window.refresh();
        self.close(cx);
    }

    /// A choice card: a small picture of the app in `p`, then its name.
    fn card(
        &self,
        ix: usize,
        title: SharedString,
        p: &Palette,
        choice: ColorChoice,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = colors(cx);
        let k = Colors::new(p, self.editing_dark);
        let on = self.choice() == choice;
        let bar = |w: f32, c: Hsla| div().w(px(w)).h(px(5.)).rounded(px(2.)).bg(c);
        let row = |sel: bool, unread: bool| {
            div()
                .h(px(16.))
                .px(px(5.))
                .flex()
                .gap(px(4.))
                .items_center()
                .rounded(px(3.))
                .when(sel, |d| d.bg(k.selection))
                .child(ui::dot(unread.then_some(k.accent), 4.))
                .child(bar(
                    if unread { 46. } else { 38. },
                    if unread { k.fg } else { k.muted_fg },
                ))
        };
        let picture = div()
            .h(px(64.))
            .flex()
            .overflow_hidden()
            .rounded(px(7.))
            .border_1()
            .border_color(k.border)
            .bg(k.bg)
            .child(
                div()
                    .w(px(16.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(6.))
                    .gap(px(4.))
                    .bg(k.chrome)
                    .child(div().size(px(8.)).rounded(px(2.)).bg(k.accent))
                    .child(div().size(px(6.)).rounded(px(2.)).bg(k.muted_fg.opacity(0.4))),
            )
            .child(
                div()
                    .flex_1()
                    .p(px(5.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(row(true, true))
                    .child(row(false, false))
                    .child(
                        div()
                            .mt(px(2.))
                            .px(px(5.))
                            .child(div().w(px(24.)).h(px(9.)).rounded(px(2.)).bg(k.accent)),
                    ),
            );
        div()
            .id(("appearance-card", ix))
            .flex_1()
            .min_w_0()
            .p(px(3.))
            .flex()
            .flex_col()
            .gap(px(5.))
            .rounded(px(10.))
            .border_2()
            .border_color(if on { ui.accent } else { ui.accent.opacity(0.) })
            .cursor_pointer()
            .hover(|s| s.bg(ui.hover))
            .on_click(cx.listener(move |this, _, window, cx| this.pick(choice, window, cx)))
            .child(picture)
            .child(
                div()
                    .px(px(2.))
                    .text_size(px(ui::TEXT_SM))
                    .text_color(if on { ui.fg } else { ui.muted_fg })
                    .when(on, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .truncate()
                    .child(title),
            )
            .into_any_element()
    }

    fn colours_tab(&self, cx: &mut Context<Self>) -> Div {
        let c = *colors(cx);
        let dark = self.editing_dark;
        let pref = self.draft.theme;
        let mode_row = segments(
            [
                ("System", pref == ThemePref::System),
                ("Light", pref == ThemePref::Light),
                ("Dark", pref == ThemePref::Dark),
            ],
            "appearance-mode",
            &c,
            cx,
            |this, ix, w, cx| {
                let pref = [ThemePref::System, ThemePref::Light, ThemePref::Dark][ix];
                this.set_mode(pref, w, cx)
            },
        );
        let edit_row = segments(
            [("Dark mode", dark), ("Light mode", !dark)],
            "appearance-edit",
            &c,
            cx,
            |this, ix, w, cx| this.edit_mode(ix == 0, w, cx),
        );

        let spec = self.draft.preset.spec();
        let preset = if dark { spec.dark } else { spec.light };
        let mut cards = vec![self.card(
            0,
            format!("{} colours", self.draft.preset.name()).into(),
            &preset,
            ColorChoice::Preset,
            cx,
        )];
        for id in ThemeId::ALL.into_iter().filter(|t| t.is_dark() == dark) {
            let ix = cards.len();
            cards.push(self.card(
                ix,
                id.title().into(),
                &id.palette(),
                ColorChoice::Theme { name: id },
                cx,
            ));
        }
        let ix = cards.len();
        let accent = derive::derive(self.draft.accent, dark);
        cards.push(self.card(ix, "Custom accent".into(), &accent, ColorChoice::Accent, cx));

        let accent = self.draft.accent;
        let accent_on = self.choice() == ColorChoice::Accent;
        let swatches = div()
            .flex()
            .gap(px(6.))
            .children(SWATCHES.iter().enumerate().map(|(ix, &(name, hue))| {
                let on = accent_on && accent.hue == hue;
                div()
                    .id(("appearance-swatch", ix))
                    .flex_1()
                    .h(px(26.))
                    .rounded(px(7.))
                    .bg(rgb(derive::swatch(hue)))
                    .border_2()
                    .border_color(if on { c.fg } else { c.fg.opacity(0.) })
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(name).build(window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.hue
                            .update(cx, |s, cx| s.set_value(f32::from(hue), window, cx));
                        this.edit_accent(|a| a.hue = hue, window, cx);
                    }))
            }));

        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(section("Mode", &c).child(mode_row))
            .child(
                section("Colours for", &c)
                    .child(edit_row)
                    .child(grid(cards, 4)),
            )
            .child(
                section("Custom accent", &c)
                    .child(swatches)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(format!("Any colour (hue {}°)", accent.hue))
                            .child(Slider::new(&self.hue).accessibility_label("Accent hue")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(24.))
                            .child(
                                Switch::new("appearance-muted")
                                    .checked(accent.chroma == Chroma::Muted)
                                    .label("Muted colours")
                                    .on_click(cx.listener(|this, on: &bool, w, cx| {
                                        let chroma = if *on { Chroma::Muted } else { Chroma::Normal };
                                        this.edit_accent(|a| a.chroma = chroma, w, cx)
                                    })),
                            )
                            .child(
                                Switch::new("appearance-tint")
                                    .checked(accent.tint)
                                    .label("Tint backgrounds")
                                    .on_click(cx.listener(|this, on: &bool, w, cx| {
                                        let on = *on;
                                        this.edit_accent(|a| a.tint = on, w, cx)
                                    })),
                            ),
                    ),
            )
            .pb(px(14.))
    }

    /// Contrast of the edited palette's text, muted text and accent on its background.
    fn readout(&self) -> String {
        let p = theme::palette(&self.draft, self.editing_dark);
        let ratio = |a: u32| format!("{:.1}:1", theme::contrast(a, p.bg));
        format!(
            "Text {} · muted text {} · accent {}",
            ratio(p.fg),
            ratio(p.muted_fg),
            ratio(p.accent)
        )
    }

    fn fonts_tab(&self, cx: &mut Context<Self>) -> Div {
        // Each chip draws in its own font: register them all (once).
        crate::fonts::register_all(cx);
        let c = *colors(cx);
        let f = Fonts::from_settings(&self.draft);
        let chips = |id: &'static str,
                         list: &'static [FontInfo],
                         chosen: &'static FontInfo,
                         set: fn(&mut Settings, &'static str)| {
            let items = list
                .iter()
                .enumerate()
                .map(|(ix, font)| {
                    let on = std::ptr::eq(font, chosen);
                    // A bare cell, like `grid`'s fillers: padding inside it
                    // would widen the cell in a short last row.
                    let chip = div()
                        .id((id, ix))
                        .w_full()
                        .h(px(30.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(if on { c.accent } else { c.border })
                        .when(on, |d| d.bg(c.selection))
                        .when(!on, |d| d.hover(|s| s.bg(c.hover)))
                        .cursor_pointer()
                        .font_family(font.family)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.edit(|s| set(s, font.id), w, cx)
                        }))
                        .child(div().min_w_0().truncate().child(font.title));
                    div().flex_1().min_w_0().child(chip).into_any_element()
                })
                .collect();
            grid(items, 3)
        };
        let ui_chips = chips("appearance-ui-font", &UI_FONTS, f.ui, |s, id| {
            s.ui_font = id.to_owned()
        });
        let code_chips = chips("appearance-code-font", &CODE_FONTS, f.mono, |s, id| {
            s.mono_font = id.to_owned()
        });
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(section("Interface font", &c).child(ui_chips))
            .child(section("Code font", &c).child(code_chips))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        Switch::new("appearance-plain-mono")
                            .checked(self.draft.plain_text_mono)
                            .label("Show plain-text mail in the code font")
                            .on_click(cx.listener(|this, on: &bool, w, cx| {
                                let on = *on;
                                this.edit(|s| s.plain_text_mono = on, w, cx)
                            })),
                    )
                    .when(f.can_mask(), |d| {
                        d.child(
                            Switch::new("appearance-mask")
                                .checked(f.mask_secrets)
                                .label("Hide secrets in code")
                                .on_click(cx.listener(|this, on: &bool, w, cx| {
                                    let on = *on;
                                    this.edit(|s| s.mask_secrets = on, w, cx)
                                })),
                        )
                    })
                    .when(self.draft.preset.spec().display_font.is_some(), |d| {
                        d.child(
                            Switch::new("appearance-headings")
                                .checked(self.draft.headings_use_ui_font)
                                .label("Use my font for headings too")
                                .on_click(cx.listener(|this, on: &bool, w, cx| {
                                    let on = *on;
                                    this.edit(|s| s.headings_use_ui_font = on, w, cx)
                                })),
                        )
                    }),
            )
            .child(section("Sample", &c).child(sample(&self.draft, &f, &c)))
            .pb(px(14.))
    }
}

/// A heading and the controls below it.
fn section(title: &'static str, c: &Colors) -> Div {
    div().flex().flex_col().gap(px(6.)).child(
        div()
            .text_size(px(ui::TEXT_XS))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.muted_fg)
            .child(title),
    )
}

/// `items` in rows of `n`, padded so every cell has the same width.
fn grid(mut items: Vec<AnyElement>, n: usize) -> Div {
    while !items.len().is_multiple_of(n) {
        items.push(div().flex_1().min_w_0().into_any_element());
    }
    let mut rows = div().flex().flex_col().gap(px(6.));
    let mut items = items.into_iter();
    loop {
        let row: Vec<AnyElement> = items.by_ref().take(n).collect();
        if row.is_empty() {
            break rows;
        }
        rows = rows.child(div().flex().gap(px(6.)).children(row));
    }
}

/// A segmented control; `on_pick` gets the index.
fn segments<const N: usize>(
    items: [(&'static str, bool); N],
    id: &'static str,
    c: &Colors,
    cx: &mut Context<Appearance>,
    on_pick: fn(&mut Appearance, usize, &mut Window, &mut Context<Appearance>),
) -> Div {
    div()
        .flex()
        .p(px(3.))
        .gap(px(2.))
        .rounded(px(7.))
        .bg(c.chrome)
        .children(items.into_iter().enumerate().map(|(ix, (label, on))| {
            div()
                .id((id, ix))
                .flex_1()
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .text_size(px(ui::TEXT_SM))
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(c.selection)
                        .text_color(c.fg)
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!on, |d| d.text_color(c.muted_fg).hover(|s| s.bg(c.hover)))
                .on_click(cx.listener(move |this, _, w, cx| on_pick(this, ix, w, cx)))
                .child(label)
        }))
}

/// A list row, a subject line and a plain-text line in the chosen fonts.
fn sample(s: &Settings, f: &Fonts, c: &Colors) -> impl IntoElement {
    let display = (!s.headings_use_ui_font)
        .then_some(s.preset.spec().display_font)
        .flatten();
    let token = "export ANTHROPIC_API_KEY=sk-ant-api03-x7Kq2mN9pR4tW8vY3zB6cD1fH5jL0sA2";
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(10.))
        .rounded(px(8.))
        .bg(c.bg)
        .border_1()
        .border_color(c.border)
        .child(
            div()
                .h(px(56.))
                .px(px(12.))
                .flex()
                .gap(px(10.))
                .items_center()
                .rounded(px(6.))
                .bg(c.selection)
                .child(ui::dot(Some(c.accent), 8.))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(c.fg)
                                .when_some(display, |d, f| d.font_family(f))
                                .truncate()
                                .child("Anna Weber"),
                        )
                        .child(
                            div()
                                .text_size(px(ui::TEXT_SM))
                                .text_color(c.muted_fg)
                                .truncate()
                                .child("Re: Werkstudent interview on Thursday · room 2.14"),
                        ),
                ),
        )
        .child(
            div()
                .text_size(px(ui::TEXT_XL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c.fg)
                .when_some(display, |d, f| d.font_family(f))
                .child("Your order #40412 has shipped"),
        )
        .child(
            div()
                .px(px(8.))
                .py(px(6.))
                .rounded(px(6.))
                .bg(c.chrome)
                .font_family(f.code().family)
                .text_size(px(ui::TEXT_SM))
                .text_color(c.fg)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(token),
        )
}

impl Render for Appearance {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = *colors(cx);
        let tab = self.tab;
        let tabs = segments(
            [("Colours", tab == Tab::Colours), ("Fonts", tab == Tab::Fonts)],
            "appearance-tab",
            &c,
            cx,
            |this, ix, _, cx| {
                this.tab = if ix == 0 { Tab::Colours } else { Tab::Fonts };
                cx.notify();
            },
        );
        let (body, note) = match tab {
            Tab::Colours => (self.colours_tab(cx), self.readout()),
            Tab::Fonts => (
                self.fonts_tab(cx),
                "Fonts: SIL Open Font License 1.1".to_owned(),
            ),
        };
        let max_h = window.viewport_size().height - px(72.);
        let panel = div()
            .id("appearance")
            .key_context("TidemailAppearance")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OpenThread, w, cx| this.done(w, cx)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .w(px(600.))
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
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(ui::TEXT_LG))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Appearance"),
                            )
                            .child(
                                div()
                                    .text_size(px(ui::TEXT_SM))
                                    .text_color(c.muted_fg)
                                    .child("One choice for dark mode and one for light. System follows Windows."),
                            ),
                    )
                    .child(tabs),
            )
            .child(
                div()
                    .id("appearance-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(18.))
                    .child(body),
            )
            .child(
                div()
                    .p(px(14.))
                    .flex()
                    .gap(px(8.))
                    .justify_end()
                    .items_center()
                    .border_t_1()
                    .border_color(c.border)
                    .child(
                        div()
                            .id("appearance-note")
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(ui::TEXT_SM))
                            .text_color(c.muted_fg)
                            .child(note),
                    )
                    .child(ui::button("appearance-cancel", "Cancel", false, &c).on_click(
                        cx.listener(|this, _, w, cx| this.cancel(w, cx)),
                    ))
                    .child(ui::button("appearance-done", "Done", true, &c).on_click(
                        cx.listener(|this, _, w, cx| this.done(w, cx)),
                    )),
            );
        deferred(
            div()
                .id("appearance-layer")
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
