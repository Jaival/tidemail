//! Colours and fonts. The preset gives a dark and a light [`Palette`]; the
//! theme mode follows Windows unless overridden in settings. The palette is
//! written over the `gpui-component` theme (so components match) and kept in
//! the [`Colors`] global for the tokens that theme has no field for.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{px, rgb, App, Global, Hsla, Window, WindowAppearance};

use crate::{preset::Preset, settings::ThemePref};

/// Interface font and base size.
pub const FONT: &str = "Segoe UI Variable Text";
pub const FONT_SIZE: f32 = 13.0;

/// One preset in one mode, as `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// List and reader panes.
    pub bg: u32,
    /// Navigation (sidebar, tabs, rail), title bar and hint bar.
    pub chrome: u32,
    pub fg: u32,
    pub muted_fg: u32,
    pub border: u32,
    pub accent: u32,
    /// Text on an accent fill.
    pub accent_fg: u32,
    /// Star.
    pub warning: u32,
    pub danger: u32,
}

/// Mail bodies sit on this light surface in every preset and mode (ADR 0002).
const PAPER: u32 = 0xfdfcf9;
const PAPER_FG: u32 = 0x1d1d1f;

/// Resolved colours of the active preset and mode.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub bg: Hsla,
    pub chrome: Hsla,
    pub fg: Hsla,
    pub muted_fg: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub accent_fg: Hsla,
    /// Selected row, active nav item: tinted accent.
    pub selection: Hsla,
    pub hover: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub paper: Hsla,
    pub paper_fg: Hsla,
}

impl Global for Colors {}

fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a, ..c }
}

impl Colors {
    fn new(p: &Palette, dark: bool) -> Self {
        let fg = hsla(p.fg);
        let accent = hsla(p.accent);
        Self {
            bg: hsla(p.bg),
            chrome: hsla(p.chrome),
            fg,
            muted_fg: hsla(p.muted_fg),
            border: hsla(p.border),
            accent,
            accent_fg: hsla(p.accent_fg),
            selection: alpha(accent, if dark { 0.20 } else { 0.13 }),
            hover: alpha(fg, if dark { 0.05 } else { 0.04 }),
            warning: hsla(p.warning),
            danger: hsla(p.danger),
            paper: hsla(PAPER),
            paper_fg: hsla(PAPER_FG),
        }
    }
}

pub fn colors(cx: &App) -> &Colors {
    cx.global::<Colors>()
}

fn mode_for(pref: ThemePref, system: WindowAppearance) -> ThemeMode {
    match pref {
        ThemePref::Light => ThemeMode::Light,
        ThemePref::Dark => ThemeMode::Dark,
        ThemePref::System => match system {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => ThemeMode::Light,
        },
    }
}

pub fn apply(pref: ThemePref, preset: Preset, window: &mut Window, cx: &mut App) {
    let mode = mode_for(pref, window.appearance());
    if !cx.has_global::<Theme>() || Theme::global(cx).mode != mode {
        Theme::change(mode, Some(window), cx);
    }
    let spec = preset.spec();
    let dark = mode.is_dark();
    let c = Colors::new(if dark { &spec.dark } else { &spec.light }, dark);
    Theme::update(cx, |t| {
        t.font_family = FONT.into();
        t.font_size = px(FONT_SIZE);
        t.radius = px(6.);
        t.background = c.bg;
        t.foreground = c.fg;
        t.border = c.border;
        t.muted = c.hover;
        t.muted_foreground = c.muted_fg;
        t.accent = c.selection;
        t.accent_foreground = c.fg;
        t.primary = c.accent;
        t.primary_foreground = c.accent_fg;
        t.ring = c.accent;
        t.caret = c.accent;
        t.link = c.accent;
        t.selection = alpha(c.accent, 0.3);
        t.input = c.border;
        t.popover = c.bg;
        t.popover_foreground = c.fg;
        t.colors.list = c.bg;
        t.list_active = c.selection;
        t.list_active_border = c.accent;
        t.list_hover = c.hover;
        t.sidebar = c.chrome;
        t.sidebar_foreground = c.fg;
        t.sidebar_border = c.border;
        t.sidebar_accent = c.selection;
        t.sidebar_accent_foreground = c.fg;
        t.sidebar_primary = c.accent;
        t.sidebar_primary_foreground = c.accent_fg;
        t.title_bar = c.chrome;
        t.title_bar_border = c.border;
        t.tab_bar = c.chrome;
        t.tab_active = c.selection;
        t.tab_active_foreground = c.fg;
        t.tab_foreground = c.muted_fg;
        t.warning = c.warning;
        t.danger = c.danger;
    });
    cx.set_global(c);
}
