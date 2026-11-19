//! UI settings in `settings.json` next to the database.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::preset::Preset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

/// How many threads after the opened one get their bodies prefetched.
pub const PREFETCH_CHOICES: [u32; 5] = [5, 10, 20, 25, 50];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemePref,
    pub prefetch: u32,
    pub preset: Preset,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePref::System,
            prefetch: 10,
            preset: Preset::Tide,
        }
    }
}

pub fn path_for(db: &Path) -> PathBuf {
    db.with_file_name("settings.json")
}

impl Settings {
    /// Missing or unreadable file: defaults. Invalid prefetch: default.
    pub fn load(path: &Path) -> Self {
        let mut s: Self = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if !PREFETCH_CHOICES.contains(&s.prefetch) {
            s.prefetch = Self::default().prefetch;
        }
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }
}
