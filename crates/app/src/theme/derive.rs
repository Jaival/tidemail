//! The custom accent: one hue, from which a whole [`Palette`] is worked out.
//!
//! Every token has a fixed per-mode lightness target in OKLCH (the "Custom
//! accent" board, `docs/design/themes-canvas-values.md`), so any hue gives a
//! usable theme. Then a contrast pass moves a token's lightness away from
//! the backgrounds until it passes: text >= 7:1, muted text >= 4.5:1,
//! accent >= 3:1, text on the accent >= 4.5:1. Pure: no GPUI types.

use serde::{Deserialize, Serialize};

use super::{contrast, Palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Chroma {
    #[default]
    Normal,
    /// A calmer accent and a less tinted selection.
    Muted,
}

/// The `accent` setting: `{"hue": 268, "chroma": "normal", "tint": true}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accent {
    /// 0-359 degrees (OKLCH hue).
    pub hue: u16,
    pub chroma: Chroma,
    /// Tint backgrounds and text with the hue (off: neutral grey).
    pub tint: bool,
}

impl Default for Accent {
    /// Indigo.
    fn default() -> Self {
        Self {
            hue: 268,
            chroma: Chroma::Normal,
            tint: true,
        }
    }
}

/// The picker's swatches: name and hue.
pub const SWATCHES: [(&str, u16); 12] = [
    ("Indigo", 268),
    ("Blue", 250),
    ("Sky", 230),
    ("Cyan", 210),
    ("Teal", 185),
    ("Green", 150),
    ("Lime", 125),
    ("Amber", 75),
    ("Orange", 50),
    ("Red", 25),
    ("Pink", 355),
    ("Violet", 300),
];

/// A swatch colour: the hue at OKLCH lightness 0.66, chroma 0.16.
pub fn swatch(hue: u16) -> u32 {
    Lch(0.66, 0.16, f32::from(hue)).rgb()
}

/// OKLCH: lightness 0-1, chroma, hue in degrees.
#[derive(Debug, Clone, Copy)]
struct Lch(f32, f32, f32);

impl Lch {
    /// `0xRRGGBB`; chroma is reduced until the colour fits in sRGB.
    fn rgb(self) -> u32 {
        let Lch(l, c, h) = self;
        let (cos, sin) = (h.to_radians().cos(), h.to_radians().sin());
        let at = |c: f32| oklab_to_srgb(l, c * cos, c * sin);
        let in_gamut = |v: [f32; 3]| v.iter().all(|x| (-0.0005..=1.0005).contains(x));
        let mut v = at(c);
        if !in_gamut(v) {
            let (mut lo, mut hi) = (0., c);
            for _ in 0..20 {
                let mid = (lo + hi) / 2.;
                if in_gamut(at(mid)) {
                    lo = mid
                } else {
                    hi = mid
                }
            }
            v = at(lo);
        }
        let [r, g, b] = v.map(|x| (x.clamp(0., 1.) * 255.).round() as u32);
        (r << 16) | (g << 8) | b
    }
}

/// OKLab -> gamma-encoded sRGB (Björn Ottosson's matrices), not clamped.
fn oklab_to_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_35 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let lin = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_4 * s3,
        -0.004_196_086 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    lin.map(|v| {
        if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.max(0.).powf(1. / 2.4) - 0.055
        }
    })
}

/// Move `c`'s lightness away from the backgrounds (up in dark mode, down in
/// light mode) in 0.005 steps until it is `min`:1 on all of them.
fn fit(mut c: Lch, against: &[u32], min: f32, dark: bool) -> Lch {
    let step = if dark { 0.005 } else { -0.005 };
    while !against.iter().all(|bg| contrast(c.rgb(), *bg) >= min)
        && (0. ..=1.).contains(&(c.0 + step))
    {
        c.0 += step;
    }
    c
}

/// The palette for `accent` in one mode.
pub fn derive(accent: Accent, dark: bool) -> Palette {
    let h = f32::from(accent.hue);
    let k = if accent.tint { 1. } else { 0. };
    let muted = accent.chroma == Chroma::Muted;
    // Accent and selection chroma: the board's (normal), calmer (muted).
    let (ac, sel_c) = match (dark, muted) {
        (true, false) => (0.16, 0.06),
        (true, true) => (0.09, 0.035),
        (false, false) => (0.17, 0.035),
        (false, true) => (0.10, 0.02),
    };
    let tone = |l: f32, c: f32| Lch(l, c * k, h);
    let pick = |d: f32, l: f32| if dark { d } else { l };

    let bg = tone(pick(0.19, 0.995), pick(0.014, 0.002)).rgb();
    let chrome = tone(pick(0.165, 0.972), pick(0.014, 0.008)).rgb();
    let border = tone(pick(0.27, 0.91), pick(0.014, 0.01)).rgb();
    let selected = Lch(pick(0.29, 0.95), sel_c, h).rgb();
    let panes = [bg, chrome, selected];

    let fg = fit(tone(pick(0.94, 0.22), pick(0.006, 0.02)), &panes, 7., dark).rgb();
    let muted_fg = fit(tone(pick(0.71, 0.50), pick(0.014, 0.016)), &panes, 4.5, dark).rgb();

    // Text on the accent: near-black on a light accent (dark mode), white on
    // a deep one (light mode). The accent moves until both rules pass.
    let accent_fg = if dark {
        Lch(0.2, 0.03, h).rgb()
    } else {
        0xffffff
    };
    let a = fit(Lch(pick(0.70, 0.55), ac, h), &panes, 3., dark);
    let accent_c = fit(a, &[accent_fg], 4.5, dark).rgb();

    // Fixed hues (star amber, danger red), lifted until they read.
    let warning = fit(Lch(pick(0.80, 0.62), 0.15, 75.), &panes, 3., dark).rgb();
    let danger = fit(Lch(pick(0.68, 0.52), 0.18, 25.), &panes, 4.5, dark).rgb();

    Palette {
        bg,
        chrome,
        fg,
        muted_fg,
        border,
        accent: accent_c,
        accent_fg,
        selected: Some(selected),
        warning,
        danger,
    }
}
