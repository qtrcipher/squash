//! `presets.toml` schema — user presets only (docs/06 §2 "Preset").
//!
//! Built-ins (`builtin:fast|balanced|max`) are code-defined in
//! [`crate::presets::PRESET_TABLE`] and never written to disk. Per the open
//! owner question (docs/06 §2 ⚠), user presets are creatable by editing the
//! file or via CLI in v1; the GUI lists them but ships no editor.

use super::SCHEMA_VERSION;
use crate::format::Format;
use crate::presets::level_bounds;
use serde::{Deserialize, Serialize};

/// Growth bound (docs/06 §5): user presets capped at 100.
pub const MAX_USER_PRESETS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPreset {
    pub version: u32,
    /// `user:<uuidv4>`; builtin ids are reserved.
    pub id: String,
    pub name: String,
    /// Create-capable formats only; must exist in the format registry.
    pub format: String,
    /// Per-format bounds (docs/06 §2): zip 1–9, 7z 0–9, tar.gz 1–9,
    /// tar.zst 1–22. Out-of-range clamps at load time (persistence phase).
    pub level: u8,
    /// RFC 3339 UTC.
    pub created_at: String,
}

impl UserPreset {
    /// Validation stub (docs/06 §2 rules). Clamping and case-insensitive
    /// uniqueness checks happen in the persistence layer.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != SCHEMA_VERSION {
            return Err(format!("unsupported preset version {}", self.version));
        }
        if !self.id.starts_with("user:") {
            return Err(format!("preset id {:?} must start with \"user:\"", self.id));
        }
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err("preset name must be 1–40 chars".to_string());
        }
        if name.chars().any(char::is_control) {
            return Err("preset name must not contain control chars".to_string());
        }
        let format: Format = self
            .format
            .parse()
            .map_err(|_| format!("unknown format {:?}", self.format))?;
        if !format.can_create() {
            return Err(format!("format {:?} is not create-capable", self.format));
        }
        let (lo, hi) = level_bounds(format).expect("create-capable formats have bounds");
        if !(lo..=hi).contains(&self.level) {
            return Err(format!("level {} outside {lo}..={hi}", self.level));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UserPreset {
        UserPreset {
            version: 1,
            id: "user:3f6b1c52-0000-4000-8000-000000000000".into(),
            name: "Web assets".into(),
            format: "tar.zst".into(),
            level: 19,
            created_at: "2026-08-13T00:00:00Z".into(),
        }
    }

    #[test]
    fn valid_sample_passes() {
        sample().validate().unwrap();
    }

    #[test]
    fn validation_rules() {
        assert!(UserPreset {
            id: "builtin:fast".into(),
            ..sample()
        }
        .validate()
        .is_err());
        assert!(UserPreset {
            name: String::new(),
            ..sample()
        }
        .validate()
        .is_err());
        assert!(UserPreset {
            name: "x".repeat(41),
            ..sample()
        }
        .validate()
        .is_err());
        assert!(UserPreset {
            format: "rar".into(),
            ..sample()
        }
        .validate()
        .is_err());
        assert!(UserPreset {
            level: 23,
            ..sample()
        }
        .validate()
        .is_err());
    }
}
