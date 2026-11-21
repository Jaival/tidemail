//! Colours (ADR 0002, ADR 0003). Each mode has one colour choice: the
//! preset's own palette, a built-in theme ([`ThemeId`]) or a palette derived
//! from the custom accent ([`derive`]). The theme mode follows Windows unless
//! overridden in settings. The palette is written over the `gpui-component`
//! theme (so components match) and kept in the [`Colors`] global for the
//! tokens that theme has no field for.

pub mod derive;
mod themes;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{px, rgb, App, Global, Hsla, Window, WindowAppearance};

use crate::{
    fonts,
    settings::{ColorChoice, Settings, ThemePref},
};

pub use themes::ThemeId;

/// Base text size.
pub const FONT_SIZE: f32 = 13.0;

/// One palette, as `0xRRGGBB`.
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
    /// Selected row, active nav item. `None`: the accent at a low alpha.
    pub selected: Option<u32>,
    /// Star.
    pub warning: u32,
    pub danger: u32,
}

/// Mail bodies sit on this light surface in every preset and mode (ADR 0002).
const PAPER: u32 = 0xfdfcf9;
const PAPER_FG: u32 = 0x1d1d1f;

/// Resolved colours of the active choice and mode.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub bg: Hsla,
    pub chrome: Hsla,
    pub fg: Hsla,
    pub muted_fg: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub accent_fg: Hsla,
    /// Selected row, active nav item.
    pub selection: Hsla,
    pub hover: Hsla,
    /// Text field fill: the border colour, or `bg` where a placeholder would not
    /// read on the border (High contrast).
    pub field: Hsla,
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

/// Alpha of the accent tint used as the selection when a palette has none.
fn selection_alpha(dark: bool) -> f32 {
    if dark {
        0.20
    } else {
        0.13
    }
}

impl Colors {
    pub fn new(p: &Palette, dark: bool) -> Self {
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
            selection: p
                .selected
                .map_or_else(|| alpha(accent, selection_alpha(dark)), hsla),
            hover: alpha(fg, if dark { 0.05 } else { 0.04 }),
            field: hsla(if contrast(p.muted_fg, p.border) >= 3. {
                p.border
            } else {
                p.bg
            }),
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

/// Is the window dark for this preference?
pub fn is_dark(pref: ThemePref, system: WindowAppearance) -> bool {
    match pref {
        ThemePref::Light => false,
        ThemePref::Dark => true,
        ThemePref::System => matches!(
            system,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    }
}

/// The palette the settings choose for one mode.
pub fn palette(settings: &Settings, dark: bool) -> Palette {
    let choice = if dark {
        settings.colors_dark
    } else {
        settings.colors_light
    };
    match choice {
        ColorChoice::Preset => {
            let spec = settings.preset.spec();
            if dark {
                spec.dark
            } else {
                spec.light
            }
        }
        ColorChoice::Theme { name } => name.palette(),
        ColorChoice::Accent => derive::derive(settings.accent, dark),
    }
}

/// The selection a palette shows over `bg`, as an opaque colour (for contrast
/// checks): `selected`, or the accent tint blended over `bg`.
#[cfg(test)]
pub fn selected_on_bg(p: &Palette, dark: bool) -> u32 {
    p.selected
        .unwrap_or_else(|| blend(p.accent, p.bg, selection_alpha(dark)))
}

fn channels(c: u32) -> [f32; 3] {
    [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff].map(|v| v as f32 / 255.)
}

#[cfg(test)]
fn blend(top: u32, under: u32, a: f32) -> u32 {
    let (t, u) = (channels(top), channels(under));
    let v: [u32; 3] = std::array::from_fn(|i| ((t[i] * a + u[i] * (1. - a)) * 255.).round() as u32);
    (v[0] << 16) | (v[1] << 8) | v[2]
}

/// WCAG relative luminance of `0xRRGGBB`.
pub fn luminance(c: u32) -> f32 {
    let [r, g, b] = channels(c).map(|v| {
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio, 1 to 21.
pub fn contrast(a: u32, b: u32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Apply the settings' colours and fonts in the mode the preference picks.
pub fn apply(settings: &Settings, window: &mut Window, cx: &mut App) {
    let dark = is_dark(settings.theme, window.appearance());
    apply_mode(settings, dark, window, cx);
}

/// Apply in a given mode (the Appearance overlay shows the mode it edits).
pub fn apply_mode(settings: &Settings, dark: bool, window: &mut Window, cx: &mut App) {
    let mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    if !cx.has_global::<Theme>() || Theme::global(cx).mode != mode {
        Theme::change(mode, Some(window), cx);
    }
    let c = Colors::new(&palette(settings, dark), dark);
    let f = fonts::Fonts::from_settings(settings);
    fonts::register(f.ui, cx);
    fonts::register(f.code(), cx);
    Theme::update(cx, |t| {
        t.font_family = f.ui.family.into();
        t.mono_font_family = f.code().family.into();
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
        t.slider_bar = c.accent;
        t.slider_thumb = c.fg;
        t.switch = c.border;
        t.switch_thumb = c.fg;
    });
    cx.set_global(c);
}

#[cfg(test)]
mod tests;
