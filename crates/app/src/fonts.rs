//! Bundled fonts (`assets/fonts`, SIL Open Font License; licence texts next
//! to the files). Only these are offered; the Windows system font is only the
//! fallback. The chosen fonts are registered when the theme is applied, once
//! each.
//!
//! All 21 are embedded (~12 MB of the exe; embedding costs no launch time,
//! measured in 1G-U3b). Only the chosen two are registered at startup
//! ([`register`]); the Fonts tab registers the rest to preview them
//! ([`register_all`]).
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
pub const UI_FONTS: [FontInfo; 11] = [
    font!("inter", "Inter", "Inter", [
        "Inter-Regular.ttf", "Inter-Medium.ttf", "Inter-SemiBold.ttf", "Inter-Bold.ttf"
    ]),
    font!("geist", "Geist", "Geist", [
        "Geist-Regular.ttf", "Geist-Medium.ttf", "Geist-SemiBold.ttf", "Geist-Bold.ttf"
    ]),
    font!("ibm-plex-sans", "IBM Plex Sans", "IBM Plex Sans", [
        "IBMPlexSans-Regular.ttf", "IBMPlexSans-Medium.ttf", "IBMPlexSans-SemiBold.ttf", "IBMPlexSans-Bold.ttf"
    ]),
    font!("figtree", "Figtree", "Figtree", [
        "Figtree-Regular.ttf", "Figtree-Medium.ttf", "Figtree-SemiBold.ttf", "Figtree-Bold.ttf"
    ]),
    font!("atkinson-hyperlegible-next", "Atkinson Hyperlegible Next", "Atkinson Hyperlegible Next", [
        "AtkinsonHyperlegibleNext-Regular.ttf",
        "AtkinsonHyperlegibleNext-Medium.ttf",
        "AtkinsonHyperlegibleNext-SemiBold.ttf",
        "AtkinsonHyperlegibleNext-Bold.ttf"
    ]),
    // No 500: Medium falls back to Regular.
    font!("source-serif-4", "Source Serif 4", "Source Serif 4", [
        "SourceSerif4-Regular.otf", "SourceSerif4-SemiBold.otf", "SourceSerif4-Bold.otf"
    ]),
    // The small-size optical cut; no 600: SemiBold falls back to Medium.
    // DirectWrite names the family "Google Sans Text Text" (its typographic
    // family plus the "Text" optical-size name); the test below checks it.
    font!("google-sans", "Google Sans Text Text", "Google Sans", [
        "GoogleSansText-Regular.ttf", "GoogleSansText-Medium.ttf", "GoogleSansText-Bold.ttf"
    ]),
    font!("space-grotesk", "Space Grotesk", "Space Grotesk", [
        "SpaceGrotesk-Regular.ttf", "SpaceGrotesk-Medium.ttf", "SpaceGrotesk-SemiBold.ttf", "SpaceGrotesk-Bold.ttf"
    ]),
    font!("google-sans-flex", "Google Sans Flex", "Google Sans Flex", [
        "GoogleSansFlex[ROND,opsz,slnt,wdth,wght].ttf"
    ]),
    font!("manrope", "Manrope", "Manrope", [
        "Manrope-Regular.ttf", "Manrope-Medium.ttf", "Manrope-SemiBold.ttf", "Manrope-Bold.ttf"
    ]),
    font!("plus-jakarta-sans", "Plus Jakarta Sans", "Plus Jakarta Sans", [
        "PlusJakartaSans-Regular.ttf",
        "PlusJakartaSans-Medium.ttf",
        "PlusJakartaSans-SemiBold.ttf",
        "PlusJakartaSans-Bold.ttf"
    ]),
];

/// Code fonts; the first is the default.
pub const CODE_FONTS: [FontInfo; 10] = [
    font!("obel-mono", "Obel Mono", "Obel Mono", ["ObelMono[wght].ttf"]),
    font!("jetbrains-mono", "JetBrains Mono", "JetBrains Mono", ["JetBrainsMono[wght].ttf"]),
    font!("geist-mono", "Geist Mono", "Geist Mono", ["GeistMono[wght].ttf"]),
    font!("ibm-plex-mono", "IBM Plex Mono", "IBM Plex Mono", [
        "IBMPlexMono-Regular.ttf", "IBMPlexMono-Medium.ttf", "IBMPlexMono-SemiBold.ttf", "IBMPlexMono-Bold.ttf"
    ]),
    font!("source-code-pro", "Source Code Pro", "Source Code Pro", ["SourceCodePro[wght].ttf"]),
    font!("atkinson-hyperlegible-mono", "Atkinson Hyperlegible Mono", "Atkinson Hyperlegible Mono", [
        "AtkinsonHyperlegibleMono[wght].ttf"
    ]),
    // Only regular and bold exist.
    font!("space-mono", "Space Mono", "Space Mono", ["SpaceMono-Regular.ttf", "SpaceMono-Bold.ttf"]),
    font!("google-sans-code", "Google Sans Code", "Google Sans Code", ["GoogleSansCode[wght].ttf"]),
    // The release files register one family per weight; only 400 ships.
    font!("commit-mono", "CommitMono 400", "Commit Mono", ["CommitMono-400-Regular.ttf"]),
    font!("monaspace-neon", "Monaspace Neon Var", "Monaspace Neon", ["Monaspace Neon Var.ttf"]),
];

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

/// Register every bundled font (the Fonts tab draws each chip in its font).
pub fn register_all(cx: &mut App) {
    for f in UI_FONTS.iter().chain(&CODE_FONTS).chain([&OBEL_MONO_MASKED]) {
        register(f, cx);
    }
}

/// The bundled font registered as `family` (the reader's webview fetches
/// the code font by family name).
pub fn by_family(family: &str) -> Option<&'static FontInfo> {
    UI_FONTS
        .iter()
        .chain(&CODE_FONTS)
        .chain([&OBEL_MONO_MASKED])
        .find(|f| f.family == family)
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
    fn the_table_has_21_fonts_with_unique_ids_and_ofl_licences() {
        assert_eq!(UI_FONTS.len() + CODE_FONTS.len(), 21);
        let mut ids: Vec<&str> = UI_FONTS.iter().chain(&CODE_FONTS).map(|f| f.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 21);
        for f in all() {
            assert!(f.licence.contains("SIL Open Font License, Version 1.1"), "{}", f.title);
        }
    }

    #[test]
    fn every_family_registers_under_its_name_with_a_bold() {
        let ts = text_system(&all());
        let names = ts.all_font_names();
        for f in all() {
            assert!(names.iter().any(|n| n == f.family), "{} registers {:?}", f.title, f.family);
            assert_eq!(by_family(f.family).map(|x| x.family), Some(f.family));
        }
        let text = "Review requested 0123";
        for f in &UI_FONTS {
            let font = Font {
                family: f.family.into(),
                ..Font::default()
            };
            let width = |weight| {
                let id = ts.font_id(&Font { weight, ..font.clone() }).unwrap();
                ts.layout_line(text, px(13.), &[FontRun { len: text.len(), font_id: id }])
                    .width
            };
            assert_ne!(width(FontWeight::BOLD), width(FontWeight::NORMAL), "{} has a bold", f.title);
        }
    }

    /// GPUI centres a line's ascent + descent in its line height (default
    /// 1.618 em, which every text style here keeps); each ASCII letter's ink
    /// must stay inside that box, or a fixed-height row clips it.
    #[test]
    fn letters_fit_their_line_box() {
        let ts = text_system(&all());
        let chars: Vec<char> = (' '..='~').collect();
        let text: String = chars.iter().collect();
        let mut misses = Vec::new();
        for f in all() {
            for weight in [FontWeight::NORMAL, FontWeight::SEMIBOLD] {
                let id = ts
                    .font_id(&Font { family: f.family.into(), weight, ..Font::default() })
                    .unwrap();
                let upm = ts.font_metrics(id).units_per_em as f32;
                let (mut up, mut down) = (0f32, 0f32);
                for &c in &chars {
                    let Some(glyph) = ts.glyph_for_char(id, c) else { continue };
                    let b = ts.typographic_bounds(id, glyph).unwrap();
                    up = up.max((b.origin.y + b.size.height) / upm);
                    down = down.max(-b.origin.y / upm);
                }
                for size in [11., 12., 13., 15., 19.].map(px) {
                    let line = size * 1.618;
                    let layout =
                        ts.layout_line(&text, size, &[FontRun { len: text.len(), font_id: id }]);
                    let pad = (line - layout.ascent - layout.descent) / 2.;
                    let top = pad + layout.ascent - size * up;
                    let bottom = pad + layout.ascent + size * down;
                    if top < px(-0.5) || bottom > line + px(0.5) {
                        misses.push(format!("{} {weight:?} {size:?}: ink {top:?}..{bottom:?} of {line:?}", f.title));
                    }
                }
            }
        }
        assert!(misses.is_empty(), "letters outside their line box:
{}", misses.join("
"));
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
