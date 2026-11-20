use super::derive::{derive, Accent, Chroma, SWATCHES};
use super::*;
use crate::preset::Preset;

/// Minimum ratios: text, muted text, accent, text on the accent.
struct Rules {
    fg: f32,
    muted: f32,
    accent: f32,
    on_accent: f32,
}

const NORMAL: Rules = Rules {
    fg: 7.,
    muted: 4.5,
    accent: 3.,
    on_accent: 4.5,
};

/// High contrast's own targets (on its black panes).
const HIGH: Rules = Rules {
    fg: 15.,
    muted: 10.,
    accent: 10.,
    on_accent: 10.,
};

/// Every rule on every pane; returns the failures.
fn check(name: &str, p: &Palette, dark: bool, r: &Rules) -> Vec<String> {
    let sel = selected_on_bg(p, dark);
    let mut bad = Vec::new();
    let mut need = |what: &str, fg: u32, bg: u32, min: f32| {
        let got = contrast(fg, bg);
        if got < min {
            bad.push(format!(
                "{name}: {what} {fg:06x} on {bg:06x} is {got:.2}, needs {min}"
            ));
        }
    };
    for (pane, bg, scale) in [("bg", p.bg, 1.), ("chrome", p.chrome, 1.), ("selected", sel, 0.)] {
        // On the selection High contrast keeps the normal rules.
        let pick = |high: f32, normal: f32| if scale == 1. { high } else { normal };
        need(&format!("fg/{pane}"), p.fg, bg, pick(r.fg, NORMAL.fg));
        need(&format!("muted/{pane}"), p.muted_fg, bg, pick(r.muted, NORMAL.muted));
        need(&format!("accent/{pane}"), p.accent, bg, pick(r.accent, NORMAL.accent));
    }
    need("accent_fg/accent", p.accent_fg, p.accent, r.on_accent);
    for (pane, bg) in [("bg", p.bg), ("chrome", p.chrome)] {
        need(&format!("warning/{pane}"), p.warning, bg, 3.);
        need(&format!("danger/{pane}"), p.danger, bg, 4.5);
    }
    bad
}

#[test]
fn built_in_themes_pass_the_contrast_rules() {
    let mut bad = Vec::new();
    for id in ThemeId::ALL {
        let rules = if id == ThemeId::HighContrast { &HIGH } else { &NORMAL };
        bad.extend(check(id.title(), &id.palette(), id.is_dark(), rules));
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn preset_palettes_pass_the_contrast_rules() {
    let mut bad = Vec::new();
    for preset in Preset::ALL {
        let spec = preset.spec();
        bad.extend(check(&format!("{} dark", preset.name()), &spec.dark, true, &NORMAL));
        bad.extend(check(&format!("{} light", preset.name()), &spec.light, false, &NORMAL));
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn every_derived_accent_passes_the_contrast_rules() {
    let mut bad = Vec::new();
    for hue in (0..360).step_by(5) {
        for chroma in [Chroma::Normal, Chroma::Muted] {
            for tint in [true, false] {
                for dark in [true, false] {
                    let accent = Accent { hue, chroma, tint };
                    let name = format!("{accent:?} dark={dark}");
                    bad.extend(check(&name, &derive(accent, dark), dark, &NORMAL));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} failures:\n{}", bad.len(), bad.join("\n"));
}

fn hue_of(c: u32) -> f32 {
    let h: Hsla = rgb(c).into();
    h.h * 360.
}

#[test]
fn derived_accent_keeps_its_hue() {
    for (_, hue) in SWATCHES {
        let p = derive(Accent { hue, ..Accent::default() }, true);
        let swatch_hue = hue_of(derive::swatch(hue));
        let d = (hue_of(p.accent) - swatch_hue).abs();
        assert!(d.min(360. - d) < 12., "hue {hue}: {:.0} vs {swatch_hue:.0}", hue_of(p.accent));
    }
}

#[test]
fn tint_off_is_grey_and_muted_is_calmer() {
    let grey = derive(Accent { tint: false, ..Accent::default() }, true);
    let [r, g, b] = channels(grey.bg);
    assert!((r - g).abs() < 0.01 && (g - b).abs() < 0.01, "{:06x}", grey.bg);
    let sat = |c: u32| Hsla::from(rgb(c)).s;
    let normal = derive(Accent::default(), false);
    let muted = derive(Accent { chroma: Chroma::Muted, ..Accent::default() }, false);
    assert!(sat(muted.accent) < sat(normal.accent));
}

#[test]
fn choice_picks_the_palette() {
    let mut s = Settings::default();
    assert_eq!(palette(&s, true), Preset::Tide.spec().dark);
    s.colors_dark = ColorChoice::Theme { name: ThemeId::Fjord };
    s.colors_light = ColorChoice::Accent;
    assert_eq!(palette(&s, true), ThemeId::Fjord.palette());
    assert_eq!(palette(&s, false), derive(s.accent, false));
    // The preset still changes the layout, not a chosen theme.
    s.preset = Preset::Paper;
    assert_eq!(palette(&s, true), ThemeId::Fjord.palette());
}

#[test]
fn derive_is_fast() {
    let t = std::time::Instant::now();
    for hue in 0..360 {
        std::hint::black_box(derive(Accent { hue, ..Accent::default() }, hue % 2 == 0));
    }
    // 360 palettes; one is derived per apply.
    assert!(t.elapsed().as_millis() < 500, "{:?}", t.elapsed());
}
