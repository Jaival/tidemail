//! Layout presets (docs/adr/0002-layout-presets.md). A preset is data: the
//! shell, list and reader read the spec of the active preset.

use serde::{Deserialize, Serialize};

use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    #[default]
    Tide,
    Command,
    Paper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavStyle {
    Sidebar,
    Tabs,
    Rail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // Names as in ADR 0002.
pub enum RowStyle {
    TwoLine,
    OneLine,
    ThreeLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderPlacement {
    /// Right pane, always on screen.
    Split,
    /// Replaces the list while `AppState::reading`.
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetSpec {
    pub nav: NavStyle,
    pub row: RowStyle,
    pub row_height: f32,
    pub reader: ReaderPlacement,
    /// j/k open the thread (else they only move the selection, unless reading).
    pub open_on_move: bool,
    pub hint_bar: bool,
    /// Font for the subject and list title. `None`: the interface font.
    pub display_font: Option<&'static str>,
    /// Widths of the navigation and list panes (0: not a pane).
    pub nav_width: f32,
    pub list_width: f32,
    pub dark: Palette,
    pub light: Palette,
}

const TIDE: PresetSpec = PresetSpec {
    nav: NavStyle::Sidebar,
    row: RowStyle::TwoLine,
    row_height: 56.0,
    reader: ReaderPlacement::Split,
    open_on_move: true,
    hint_bar: false,
    display_font: None,
    nav_width: 208.0,
    list_width: 400.0,
    dark: Palette {
        bg: 0x111719,
        chrome: 0x0c1113,
        fg: 0xe3ebed,
        muted_fg: 0x8f9fa4,
        border: 0x1e292d,
        accent: 0x2fb5a5,
        accent_fg: 0x04201c,
        selected: None,
        warning: 0xe8b04a,
        danger: 0xef6b6b,
    },
    light: Palette {
        bg: 0xffffff,
        chrome: 0xf2f6f7,
        fg: 0x14211f,
        muted_fg: 0x5d6d71,
        border: 0xe1e8ea,
        accent: 0x0b7f74,
        accent_fg: 0xffffff,
        selected: None,
        warning: 0xb47a0e,
        danger: 0xc93c3c,
    },
};

const COMMAND: PresetSpec = PresetSpec {
    nav: NavStyle::Tabs,
    row: RowStyle::OneLine,
    row_height: 32.0,
    reader: ReaderPlacement::Replace,
    open_on_move: false,
    hint_bar: true,
    display_font: None,
    nav_width: 0.0,
    list_width: 0.0,
    dark: Palette {
        bg: 0x0e0e13,
        chrome: 0x13131a,
        fg: 0xe4e4ee,
        muted_fg: 0x9090a5,
        border: 0x23232f,
        accent: 0x8f7ff7,
        accent_fg: 0x120c2e,
        selected: None,
        warning: 0xe8b04a,
        danger: 0xef6b6b,
    },
    light: Palette {
        bg: 0xfbfbfd,
        chrome: 0xf1f0f7,
        fg: 0x18181f,
        muted_fg: 0x66667a,
        border: 0xe3e2ed,
        accent: 0x6a4ddd,
        accent_fg: 0xffffff,
        selected: None,
        warning: 0xb47a0e,
        danger: 0xc23535,
    },
};

const PAPER: PresetSpec = PresetSpec {
    nav: NavStyle::Rail,
    row: RowStyle::ThreeLine,
    row_height: 76.0,
    reader: ReaderPlacement::Split,
    open_on_move: true,
    hint_bar: false,
    display_font: Some("Georgia"),
    nav_width: 44.0,
    list_width: 320.0,
    dark: Palette {
        bg: 0x1b1816,
        chrome: 0x151210,
        fg: 0xece4da,
        muted_fg: 0xa39787,
        border: 0x2e2823,
        accent: 0xdc7b4e,
        accent_fg: 0x2a1206,
        selected: None,
        warning: 0xe2a948,
        danger: 0xe8695f,
    },
    light: Palette {
        bg: 0xfaf7f2,
        chrome: 0xf1eae0,
        fg: 0x2a211b,
        muted_fg: 0x6e5f52,
        border: 0xe5dccd,
        accent: 0xb5532a,
        accent_fg: 0xffffff,
        selected: None,
        warning: 0xa06f12,
        danger: 0xb9382f,
    },
};

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Tide, Preset::Command, Preset::Paper];

    pub fn spec(self) -> &'static PresetSpec {
        match self {
            Preset::Tide => &TIDE,
            Preset::Command => &COMMAND,
            Preset::Paper => &PAPER,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Preset::Tide => "Tide",
            Preset::Command => "Command",
            Preset::Paper => "Paper",
        }
    }

    /// Case-insensitive name, for `--preset`.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_names() {
        assert_eq!(Preset::parse("command"), Some(Preset::Command));
        assert_eq!(Preset::parse("PAPER"), Some(Preset::Paper));
        assert_eq!(Preset::parse("console"), None);
    }

    #[test]
    fn settings_name_is_camel_case() {
        assert_eq!(serde_json::to_string(&Preset::Tide).unwrap(), "\"tide\"");
    }
}
