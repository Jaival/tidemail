//! Bundled fonts (`assets/fonts`, SIL Open Font License; licence texts next
//! to the files). Only these are offered; the Windows system font is only the
//! fallback. The chosen fonts are registered when the theme is applied, once
//! each.
//!
//! For now only the defaults are embedded: Inter and Obel Mono. The other
//! interface fonts are in `assets/fonts` already and join `UI_FONTS` when the
//! font picker grows (docs/design/themes-plan.md, Fonts).
//!
//! "Hide secrets in code" is Obel Mono's `ss01` feature. GPUI passes font
//! features down to every text run below an element, so setting `ss01` on a
//! code span could also switch on the interface font's own `ss01` (Inter's
//! alternate digits). The switch selects Obel Mono Masked instead, the
//! foundry's build with `ss01` on by default (Forkline found the same).

use std::{borrow::Cow, collections::HashSet};

use gpui_kit::{App, Global};

use crate::settings::Settings;

/// One bundled font family.
#[derive(Debug, PartialEq, Eq)]
pub struct FontInfo {
    /// The settings value, e.g. `"inter"`.
    pub id: &'static str,
    /// The family name the files register.
    pub family: &'static str,
    pub title: &'static str,
    pub files: &'static [&'static [u8]],
    pub licence: &'static str,
}

macro_rules! font {
    ($id:literal, $family:literal, $title:literal, [$($file:literal),+ $(,)?]) => {
        FontInfo {
            id: $id,
            family: $family,
            title: $title,
            files: &[$(include_bytes!(concat!("../assets/fonts/", $id, "/", $file))),+],
            licence: include_str!(concat!("../assets/fonts/", $id, "/OFL.txt")),
        }
    };
}

/// Interface fonts; the first is the default.
pub const UI_FONTS: [FontInfo; 1] = [font!(
    "inter",
    "Inter",
    "Inter",
    [
        "Inter-Regular.ttf",
        "Inter-Medium.ttf",
        "Inter-SemiBold.ttf",
        "Inter-Bold.ttf"
    ]
)];

/// Code fonts; the first is the default.
pub const CODE_FONTS: [FontInfo; 1] = [font!(
    "obel-mono",
    "Obel Mono",
    "Obel Mono",
    ["ObelMono[wght].ttf"]
)];

/// Obel Mono with `ss01` (masking) on by default.
pub const OBEL_MONO_MASKED: FontInfo = font!(
    "obel-mono",
    "Obel Mono Masked",
    "Obel Mono Masked",
    ["ObelMonoMasked[wght].ttf"]
);

/// The fonts the settings choose.
#[derive(Debug, Clone, Copy)]
pub struct Fonts {
    pub ui: &'static FontInfo,
    pub mono: &'static FontInfo,
    pub mask_secrets: bool,
}

fn find(list: &'static [FontInfo], id: &str) -> &'static FontInfo {
    list.iter().find(|f| f.id == id).unwrap_or(&list[0])
}

impl Fonts {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            ui: find(&UI_FONTS, &s.ui_font),
            mono: find(&CODE_FONTS, &s.mono_font),
            mask_secrets: s.mask_secrets,
        }
    }

    /// Only Obel Mono can hide secrets.
    pub fn can_mask(&self) -> bool {
        self.mono.id == "obel-mono"
    }

    /// The family code is drawn in.
    pub fn code(&self) -> &'static FontInfo {
        if self.can_mask() && self.mask_secrets {
            &OBEL_MONO_MASKED
        } else {
            self.mono
        }
    }
}

/// Families registered so far.
#[derive(Default)]
struct Registered(HashSet<&'static str>);

impl Global for Registered {}

/// Register `font` with the text system, once.
pub fn register(font: &'static FontInfo, cx: &mut App) {
    if !cx.default_global::<Registered>().0.insert(font.family) {
        return;
    }
    let files = font.files.iter().map(|f| Cow::Borrowed(*f)).collect();
    if let Err(e) = cx.text_system().add_fonts(files) {
        eprintln!("tidemail: cannot load the font {}: {e:#}", font.title);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::{px, Font, FontFeatures, FontRun, FontWeight, PlatformTextSystem};

    use super::*;

    fn text_system(fonts: &[&'static FontInfo]) -> Arc<dyn PlatformTextSystem> {
        let ts = gpui_kit::platform::current_platform(false).text_system();
        let files = fonts
            .iter()
            .flat_map(|f| f.files.iter().map(|b| Cow::Borrowed(*b)))
            .collect();
        ts.add_fonts(files).unwrap();
        ts
    }

    fn all() -> Vec<&'static FontInfo> {
        UI_FONTS
            .iter()
            .chain(&CODE_FONTS)
            .chain([&OBEL_MONO_MASKED])
            .collect()
    }

    #[test]
    fn every_family_registers_under_its_name() {
        let ts = text_system(&all());
        let names = ts.all_font_names();
        for f in all() {
            assert!(names.iter().any(|n| n == f.family), "{} not found", f.family);
        }
    }

    #[test]
    fn interface_font_has_its_weights() {
        let ts = text_system(&all());
        let font = Font {
            family: "Inter".into(),
            ..Font::default()
        };
        let width = |weight| {
            let id = ts.font_id(&Font { weight, ..font.clone() }).unwrap();
            ts.layout_line("Hello world 0123", px(13.), &[FontRun { len: 16, font_id: id }])
                .width
        };
        assert!(width(FontWeight::BOLD) > width(FontWeight::NORMAL));
    }

    #[test]
    fn masked_build_draws_other_glyphs_for_a_token() {
        let ts = text_system(&all());
        let token = format!("sk-ant-{}", "aB3dE6gH9jK2mN5pQ8sT1vW4yZ7bC0eF3hJ6");
        let glyphs = |family: &'static str| {
            let font = Font {
                family: family.into(),
                features: FontFeatures::default(),
                ..Font::default()
            };
            let id = ts.font_id(&font).unwrap();
            let line = ts.layout_line(&token, px(13.), &[FontRun { len: token.len(), font_id: id }]);
            line.runs
                .iter()
                .flat_map(|r| r.glyphs.iter().map(|g| g.id.0))
                .collect::<Vec<_>>()
        };
        let (plain, masked) = (glyphs("Obel Mono"), glyphs("Obel Mono Masked"));
        assert_eq!(plain.len(), masked.len());
        // The prefix keeps its glyphs; the secret after it is masked.
        assert_ne!(plain, masked);
    }

    #[test]
    fn unknown_ids_fall_back_and_mask_needs_obel() {
        let s = Settings {
            ui_font: "nope".into(),
            ..Settings::default()
        };
        let f = Fonts::from_settings(&s);
        assert_eq!(f.ui.id, "inter");
        assert_eq!(f.code().family, "Obel Mono Masked");
        let f = Fonts {
            mask_secrets: false,
            ..f
        };
        assert_eq!(f.code().family, "Obel Mono");
    }
}
