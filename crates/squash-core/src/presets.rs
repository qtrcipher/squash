//! Preset system (docs/05 §3).
//!
//! Exactly three presets — no flag jungle (docs/01 §3.3). Presets are **data,
//! not code paths**: one table maps (format, preset) → codec parameters.
//! Adding a format adds rows here; nothing else changes.
//!
//! Level bounds per docs/06 §2: zip `1–9`, 7z `0–9`, tar.gz `1–9`, tar.zst `1–22`.

use crate::format::Format;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Fast,
    #[default]
    Balanced,
    Max,
}

impl Preset {
    /// Exactly three presets, in speed order. Stability contract.
    pub const ALL: [Preset; 3] = [Self::Fast, Self::Balanced, Self::Max];

    /// Builtin preset ids as stored in settings/history (docs/06 §2).
    pub fn id(&self) -> &'static str {
        match self {
            Self::Fast => "builtin:fast",
            Self::Balanced => "builtin:balanced",
            Self::Max => "builtin:max",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }
}

/// Per-format codec parameters for one preset. Phase 1 carries the
/// compression `level` only; richer per-codec knobs are added with handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetParams {
    pub level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresetRow {
    pub format: Format,
    pub preset: Preset,
    pub params: PresetParams,
}

const fn row(format: Format, preset: Preset, level: u8) -> PresetRow {
    PresetRow {
        format,
        preset,
        params: PresetParams { level },
    }
}

/// The preset table: one row per (create-capable format × preset).
///
/// Anchors from the docs: fast tar.zst = zstd level 3 (docs/05 §3);
/// max 7z = LZMA2 high. Remaining levels sit inside docs/06 bounds and
/// strictly increase fast < balanced < max.
pub const PRESET_TABLE: &[PresetRow] = &[
    row(Format::Zip, Preset::Fast, 1),
    row(Format::Zip, Preset::Balanced, 6),
    row(Format::Zip, Preset::Max, 9),
    row(Format::SevenZ, Preset::Fast, 1),
    row(Format::SevenZ, Preset::Balanced, 5),
    row(Format::SevenZ, Preset::Max, 9),
    row(Format::TarGz, Preset::Fast, 1),
    row(Format::TarGz, Preset::Balanced, 6),
    row(Format::TarGz, Preset::Max, 9),
    row(Format::TarZst, Preset::Fast, 3),
    row(Format::TarZst, Preset::Balanced, 7),
    row(Format::TarZst, Preset::Max, 19),
];

/// Look up the parameters for a (format, preset) pair.
pub fn params(format: Format, preset: Preset) -> Option<PresetParams> {
    PRESET_TABLE
        .iter()
        .find(|r| r.format == format && r.preset == preset)
        .map(|r| r.params)
}

/// Valid level range per format (docs/06 §2). `None` for non-creatable formats.
pub fn level_bounds(format: Format) -> Option<(u8, u8)> {
    match format {
        Format::Zip => Some((1, 9)),
        Format::SevenZ => Some((0, 9)),
        Format::TarGz => Some((1, 9)),
        Format::TarZst => Some((1, 22)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_three_presets() {
        assert_eq!(Preset::ALL.len(), 3);
    }

    #[test]
    fn table_covers_create_capable_formats() {
        for format in Format::CREATE_CAPABLE {
            for preset in Preset::ALL {
                assert!(
                    params(format, preset).is_some(),
                    "missing preset row for {format}/{preset:?}"
                );
            }
        }
        assert_eq!(
            PRESET_TABLE.len(),
            Format::CREATE_CAPABLE.len() * Preset::ALL.len()
        );
    }

    #[test]
    fn levels_within_documented_bounds() {
        for row in PRESET_TABLE {
            let (lo, hi) = level_bounds(row.format).unwrap();
            assert!(
                (lo..=hi).contains(&row.params.level),
                "{:?} level {} outside {lo}..={hi}",
                row.format,
                row.params.level
            );
        }
    }

    #[test]
    fn levels_increase_with_preset() {
        for format in Format::CREATE_CAPABLE {
            let fast = params(format, Preset::Fast).unwrap().level;
            let balanced = params(format, Preset::Balanced).unwrap().level;
            let max = params(format, Preset::Max).unwrap().level;
            assert!(fast < balanced && balanced < max, "{format} not ordered");
        }
    }

    #[test]
    fn documented_anchors() {
        // docs/05 §3: "fast tar.zst = zstd level 3".
        assert_eq!(params(Format::TarZst, Preset::Fast).unwrap().level, 3);
    }

    #[test]
    fn builtin_ids_roundtrip() {
        for preset in Preset::ALL {
            assert_eq!(Preset::from_id(preset.id()), Some(preset));
        }
        assert_eq!(Preset::from_id("user:whatever"), None);
    }
}
