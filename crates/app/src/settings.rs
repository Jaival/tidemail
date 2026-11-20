//! UI settings in `settings.json` next to the database.
//!
//! Loaded field by field: a missing, unknown or malformed value loads as its
//! default and leaves the other fields alone (ADR 0003).

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::{
    fonts,
    preset::Preset,
    theme::{derive::Accent, ThemeId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

/// The colours of one mode (ADR 0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ColorChoice {
    /// The active preset's own palette.
    #[default]
    Preset,
    Theme {
        name: ThemeId,
    },
    /// Derived from `Settings::accent`.
    Accent,
}

/// How many threads after the opened one get their bodies prefetched.
pub const PREFETCH_CHOICES: [u32; 5] = [5, 10, 20, 25, 50];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: ThemePref,
    pub prefetch: u32,
    pub preset: Preset,
    pub colors_dark: ColorChoice,
    pub colors_light: ColorChoice,
    pub accent: Accent,
    /// A font id from `fonts::UI_FONTS`.
    pub ui_font: String,
    /// A font id from `fonts::CODE_FONTS`.
    pub mono_font: String,
    /// Obel Mono draws tokens and secrets as mask dots.
    pub mask_secrets: bool,
    /// The interface font replaces the preset's display font (Paper's Georgia).
    pub headings_use_ui_font: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePref::System,
            prefetch: 10,
            preset: Preset::Tide,
            colors_dark: ColorChoice::Preset,
            colors_light: ColorChoice::Preset,
            accent: Accent::default(),
            ui_font: fonts::UI_FONTS[0].id.to_owned(),
            mono_font: fonts::CODE_FONTS[0].id.to_owned(),
            mask_secrets: true,
            headings_use_ui_font: false,
        }
    }
}

pub fn path_for(db: &Path) -> PathBuf {
    db.with_file_name("settings.json")
}

/// `v[key]` as a `T`, if present and valid.
fn field<T: DeserializeOwned>(v: &Value, key: &str) -> Option<T> {
    serde_json::from_value(v.get(key)?.clone()).ok()
}

impl Settings {
    /// Missing or unreadable file: defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .map(|v| Self::from_json(&v))
            .unwrap_or_default()
    }

    pub fn from_json(v: &Value) -> Self {
        let d = Self::default();
        let font = |key: &str, list: &[fonts::FontInfo], default: String| {
            field::<String>(v, key)
                .filter(|id| list.iter().any(|f| f.id == id))
                .unwrap_or(default)
        };
        Self {
            theme: field(v, "theme").unwrap_or(d.theme),
            prefetch: field(v, "prefetch")
                .filter(|n| PREFETCH_CHOICES.contains(n))
                .unwrap_or(d.prefetch),
            preset: field(v, "preset").unwrap_or(d.preset),
            colors_dark: field(v, "colorsDark").unwrap_or(d.colors_dark),
            colors_light: field(v, "colorsLight").unwrap_or(d.colors_light),
            accent: field::<Accent>(v, "accent")
                .filter(|a| a.hue < 360)
                .unwrap_or(d.accent),
            ui_font: font("uiFont", &fonts::UI_FONTS, d.ui_font),
            mono_font: font("monoFont", &fonts::CODE_FONTS, d.mono_font),
            mask_secrets: field(v, "maskSecrets").unwrap_or(d.mask_secrets),
            headings_use_ui_font: field(v, "headingsUseUiFont").unwrap_or(d.headings_use_ui_font),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::theme::derive::Chroma;

    #[test]
    fn missing_fields_load_as_defaults() {
        assert_eq!(Settings::from_json(&json!({})), Settings::default());
        let s = Settings::from_json(&json!({ "preset": "paper" }));
        assert_eq!(s.preset, Preset::Paper);
        assert_eq!(s.colors_dark, ColorChoice::Preset);
    }

    #[test]
    fn bad_values_fall_back_one_by_one() {
        let s = Settings::from_json(&json!({
            "theme": "dark",
            "prefetch": 7,
            "colorsDark": { "kind": "theme", "name": "solarized" },
            "colorsLight": { "kind": "theme", "name": "linen" },
            "accent": { "hue": 400, "chroma": "normal", "tint": true },
            "uiFont": "comic-sans",
            "monoFont": 3,
            "maskSecrets": false,
        }));
        assert_eq!(s.theme, ThemePref::Dark);
        assert_eq!(s.prefetch, 10);
        assert_eq!(s.colors_dark, ColorChoice::Preset);
        assert_eq!(s.colors_light, ColorChoice::Theme { name: ThemeId::Linen });
        assert_eq!(s.accent, Accent::default());
        assert_eq!(s.ui_font, "inter");
        assert_eq!(s.mono_font, "obel-mono");
        assert!(!s.mask_secrets);
    }

    #[test]
    fn accent_and_choices_parse() {
        let s = Settings::from_json(&json!({
            "colorsDark": { "kind": "accent" },
            "colorsLight": { "kind": "preset" },
            "accent": { "hue": 150, "chroma": "muted", "tint": false },
        }));
        assert_eq!(s.colors_dark, ColorChoice::Accent);
        assert_eq!(
            s.accent,
            Accent {
                hue: 150,
                chroma: Chroma::Muted,
                tint: false
            }
        );
        // Missing key in the accent: the whole accent is the default.
        let s = Settings::from_json(&json!({ "accent": { "hue": 150 } }));
        assert_eq!(s.accent, Accent::default());
    }

    #[test]
    fn save_and_load_round_trip() {
        let s = Settings {
            colors_dark: ColorChoice::Theme {
                name: ThemeId::HighContrast,
            },
            colors_light: ColorChoice::Accent,
            accent: Accent {
                hue: 25,
                chroma: Chroma::Muted,
                tint: true,
            },
            headings_use_ui_font: true,
            ..Settings::default()
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["colorsDark"], json!({ "kind": "theme", "name": "high-contrast" }));
        assert_eq!(Settings::from_json(&v), s);
    }
}
