//! The eight built-in themes. Values: the "Theme options" board
//! (`docs/design/themes-canvas-values.md`), copied as CSS HSL. The board has
//! no danger colour; each mode gets one here.

use serde::{Deserialize, Serialize};

use super::Palette;

/// A built-in theme, as named in `settings.json` (`"name": "high-contrast"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeId {
    Midnight,
    Graphite,
    Fjord,
    Forest,
    Rose,
    HighContrast,
    Daylight,
    Linen,
}

/// CSS `hsl(h, s%, l%)` as `0xRRGGBB`.
const fn hsl(h: f32, s: f32, l: f32) -> u32 {
    let (s, l) = (s / 100., l / 100.);
    let c = (1. - (2. * l - 1.).abs()) * s;
    let hp = h / 60.;
    let x = c * (1. - (hp % 2. - 1.).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let m = l - c / 2.;
    (channel(r + m) << 16) | (channel(g + m) << 8) | channel(b + m)
}

const fn channel(v: f32) -> u32 {
    (v * 255. + 0.5) as u32
}

const WHITE: u32 = 0xffffff;
const DANGER_DARK: u32 = hsl(0., 85., 68.);
const DANGER_LIGHT: u32 = hsl(0., 65., 45.);

#[allow(clippy::too_many_arguments)]
const fn theme(
    bg: u32,
    chrome: u32,
    selected: u32,
    border: u32,
    fg: u32,
    muted_fg: u32,
    accent: u32,
    accent_fg: u32,
    warning: u32,
    danger: u32,
) -> Palette {
    Palette {
        bg,
        chrome,
        fg,
        muted_fg,
        border,
        accent,
        accent_fg,
        selected: Some(selected),
        warning,
        danger,
    }
}

const MIDNIGHT: Palette = theme(
    hsl(225., 12., 10.),
    hsl(225., 13., 7.5),
    hsl(228., 38., 17.5),
    hsl(225., 9., 16.5),
    hsl(220., 14., 92.),
    hsl(222., 8., 60.),
    hsl(228., 92., 69.),
    hsl(228., 45., 10.),
    hsl(42., 95., 58.),
    DANGER_DARK,
);

const GRAPHITE: Palette = theme(
    hsl(30., 4., 10.),
    hsl(30., 4., 7.5),
    hsl(36., 32., 16.),
    hsl(30., 4., 17.),
    hsl(40., 10., 91.),
    hsl(35., 5., 61.),
    hsl(36., 92., 60.),
    hsl(30., 40., 10.),
    hsl(48., 95., 60.),
    DANGER_DARK,
);

const FJORD: Palette = theme(
    hsl(218., 22., 15.),
    hsl(220., 23., 12.),
    hsl(205., 35., 23.),
    hsl(218., 18., 22.),
    hsl(215., 25., 90.),
    hsl(215., 14., 66.),
    hsl(193., 60., 62.),
    hsl(218., 30., 12.),
    hsl(40., 85., 66.),
    DANGER_DARK,
);

const FOREST: Palette = theme(
    hsl(160., 14., 9.),
    hsl(160., 16., 6.5),
    hsl(155., 35., 16.),
    hsl(160., 10., 16.),
    hsl(150., 12., 91.),
    hsl(155., 7., 61.),
    hsl(152., 60., 52.),
    hsl(160., 40., 8.),
    hsl(45., 90., 60.),
    DANGER_DARK,
);

const ROSE: Palette = theme(
    hsl(300., 10., 10.),
    hsl(300., 11., 7.5),
    hsl(330., 35., 18.),
    hsl(300., 8., 17.),
    hsl(320., 15., 92.),
    hsl(310., 7., 63.),
    hsl(336., 85., 70.),
    hsl(336., 50., 12.),
    hsl(42., 95., 62.),
    DANGER_DARK,
);

const HIGH_CONTRAST: Palette = theme(
    0x000000,
    0x000000,
    hsl(55., 100., 16.),
    hsl(0., 0., 55.),
    WHITE,
    hsl(0., 0., 82.),
    hsl(55., 100., 55.),
    0x000000,
    hsl(55., 100., 55.),
    hsl(0., 100., 72.),
);

const DAYLIGHT: Palette = theme(
    WHITE,
    hsl(225., 25., 97.),
    hsl(228., 95., 95.),
    hsl(225., 14., 90.),
    hsl(225., 25., 12.),
    hsl(222., 10., 42.),
    hsl(228., 78., 56.),
    WHITE,
    hsl(38., 95., 38.),
    DANGER_LIGHT,
);

const LINEN: Palette = theme(
    hsl(40., 30., 98.),
    hsl(40., 22., 94.),
    hsl(178., 40., 90.),
    hsl(40., 14., 86.),
    hsl(30., 15., 14.),
    hsl(30., 8., 38.),
    hsl(178., 65., 30.),
    WHITE,
    hsl(36., 95., 38.),
    DANGER_LIGHT,
);

impl ThemeId {
    pub const ALL: [ThemeId; 8] = [
        ThemeId::Midnight,
        ThemeId::Graphite,
        ThemeId::Fjord,
        ThemeId::Forest,
        ThemeId::Rose,
        ThemeId::HighContrast,
        ThemeId::Daylight,
        ThemeId::Linen,
    ];

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::Midnight => MIDNIGHT,
            ThemeId::Graphite => GRAPHITE,
            ThemeId::Fjord => FJORD,
            ThemeId::Forest => FOREST,
            ThemeId::Rose => ROSE,
            ThemeId::HighContrast => HIGH_CONTRAST,
            ThemeId::Daylight => DAYLIGHT,
            ThemeId::Linen => LINEN,
        }
    }

    pub fn is_dark(self) -> bool {
        !matches!(self, ThemeId::Daylight | ThemeId::Linen)
    }

    pub fn title(self) -> &'static str {
        match self {
            ThemeId::Midnight => "Midnight",
            ThemeId::Graphite => "Graphite",
            ThemeId::Fjord => "Fjord",
            ThemeId::Forest => "Forest",
            ThemeId::Rose => "Rosé",
            ThemeId::HighContrast => "High contrast",
            ThemeId::Daylight => "Daylight",
            ThemeId::Linen => "Linen",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_matches_css() {
        assert_eq!(hsl(0., 0., 100.), 0xffffff);
        assert_eq!(hsl(0., 100., 50.), 0xff0000);
        assert_eq!(hsl(120., 100., 25.), 0x008000);
        assert_eq!(hsl(240., 100., 50.), 0x0000ff);
        assert_eq!(hsl(300., 100., 50.), 0xff00ff);
    }

    #[test]
    fn names_round_trip() {
        for id in ThemeId::ALL {
            let name = serde_json::to_value(id).unwrap();
            assert_eq!(serde_json::from_value::<ThemeId>(name).unwrap(), id);
        }
        let hc = serde_json::to_value(ThemeId::HighContrast).unwrap();
        assert_eq!(hc, "high-contrast");
        assert!(serde_json::from_value::<ThemeId>("paper".into()).is_err());
    }
}
