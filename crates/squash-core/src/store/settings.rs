//! `settings.toml` schema (docs/06 §2 "Settings").
//!
//! Unknown keys are ignored on read (serde default) and the persistence layer
//! will preserve them on rewrite — forward-compat for mixed-version use.

use super::SCHEMA_VERSION;
use crate::presets::Preset;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "ar")]
    Ar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// Where extracted files land by default (docs/03 F3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestPolicy {
    SameFolder,
    Ask,
}

/// Anti–desktop-explosion default (docs/03 F3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LooseFilesPolicy {
    NewFolder,
    Here,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractSettings {
    pub dest_policy: DestPolicy,
    pub loose_files_policy: LooseFilesPolicy,
}

impl Default for ExtractSettings {
    fn default() -> Self {
        Self {
            dest_policy: DestPolicy::SameFolder,
            loose_files_policy: LooseFilesPolicy::NewFolder,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub language: Language,
    pub theme: Theme,
    /// Preset id, e.g. `builtin:balanced` (or a `user:<uuid>` preset).
    pub default_preset: String,
    /// Create-capable formats only (docs/06 §2): zip | 7z | tar.gz | tar.zst.
    pub default_format: String,
    pub extract: ExtractSettings,
    pub update_check_opt_in: bool,
    pub activation_counter_opt_in: bool,
    /// Drives the first-launch sheet S7 (docs/03 F1).
    pub first_launch_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            // OS-locale detection happens at load time (persistence phase);
            // the documented fallback is `en`.
            language: Language::En,
            theme: Theme::System,
            default_preset: Preset::default().id().to_string(),
            default_format: "zip".to_string(),
            extract: ExtractSettings::default(),
            update_check_opt_in: false,
            activation_counter_opt_in: false,
            first_launch_done: false,
        }
    }
}

impl Settings {
    /// Validation stub (docs/06 §2 rules). Full load-time coercion
    /// ("unknown → default + warn") ships with the persistence layer.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != SCHEMA_VERSION {
            return Err(format!("unsupported settings version {}", self.version));
        }
        if Preset::from_id(&self.default_preset).is_none()
            && !self.default_preset.starts_with("user:")
        {
            return Err(format!("unknown preset id {:?}", self.default_preset));
        }
        match self.default_format.parse::<crate::format::Format>() {
            Ok(f) if f.can_create() => {}
            _ => return Err(format!("invalid default format {:?}", self.default_format)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_data_model() {
        let s = Settings::default();
        assert_eq!(s.version, 1);
        assert_eq!(s.language, Language::En);
        assert_eq!(s.theme, Theme::System);
        assert_eq!(s.default_preset, "builtin:balanced");
        assert_eq!(s.default_format, "zip");
        assert_eq!(s.extract.dest_policy, DestPolicy::SameFolder);
        assert_eq!(s.extract.loose_files_policy, LooseFilesPolicy::NewFolder);
        assert!(!s.update_check_opt_in);
        assert!(!s.activation_counter_opt_in);
        assert!(!s.first_launch_done);
        s.validate().unwrap();
    }

    #[test]
    fn validate_rejects_bad_version_and_format() {
        let s = Settings {
            version: 99,
            ..Settings::default()
        };
        assert!(s.validate().is_err());
        let s = Settings {
            default_format: "rar".into(), // not create-capable → parse fails
            ..Settings::default()
        };
        assert!(s.validate().is_err());
    }
}
