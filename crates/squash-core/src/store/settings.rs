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
    /// Dismisses the one-time drop-zone hint on S1 (docs/03 F1: no tutorial,
    /// a single contextual hint only).
    pub drop_zone_hint_dismissed: bool,
    /// Verbose/debug logging to a local rolling log file (docs/06 §3 "Debug
    /// log"). Off by default; the GUI's S6 toggle flips it. Logs never leave
    /// the device — the user chooses to attach them to an issue.
    pub debug_logging: bool,
    /// Opt-in crash reporting (docs/06 §6). Default off; the S7 consent
    /// checkbox and the S6 toggle flip it. No consent → the Sentry client is
    /// never initialized and no crash-reporting network call is possible.
    pub crash_reporting: bool,
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
            drop_zone_hint_dismissed: false,
            debug_logging: false,
            crash_reporting: false,
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
        assert!(!s.drop_zone_hint_dismissed);
        assert!(!s.debug_logging);
        assert!(!s.crash_reporting);
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

// ---------------------------------------------------------------------------
// Persistence (docs/06 §2–§5). Load coerces unknown values to documented
// defaults with warnings; save preserves unknown keys and comments via a
// `toml_edit` overlay on the existing file.
// ---------------------------------------------------------------------------

use crate::store::{
    backup_file, declared_version_toml, read_file, write_file_atomic, LoadOutcome, StoreError,
    SETTINGS_FILE,
};
use std::path::Path;

/// Lenient mirror of [`Settings`] for load-time coercion (docs/06 §2):
/// every field optional and stringly-typed so unknown enum values survive
/// parsing and get coerced with a warning instead of failing the whole file.
#[derive(Debug, Default, Deserialize)]
struct RawSettings {
    /// Deserialized for forward-compat; version gating itself reads the raw
    /// document via `declared_version_toml` before this struct is parsed.
    #[serde(rename = "version")]
    _version: Option<i64>,
    language: Option<String>,
    theme: Option<String>,
    default_preset: Option<String>,
    default_format: Option<String>,
    extract: Option<RawExtractSettings>,
    update_check_opt_in: Option<bool>,
    activation_counter_opt_in: Option<bool>,
    first_launch_done: Option<bool>,
    drop_zone_hint_dismissed: Option<bool>,
    debug_logging: Option<bool>,
    crash_reporting: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawExtractSettings {
    dest_policy: Option<String>,
    loose_files_policy: Option<String>,
}

/// Forward-only migration chain (docs/06 §4). v1 is the first schema, so the
/// chain is empty: any older-versioned file fails migration, is backed up as
/// `<file>.v<N>.bak`, and the caller falls back to defaults. Newer-versioned
/// files are handled separately (never overwritten).
fn migrate_settings(raw: &str, from: u32) -> Result<Settings, StoreError> {
    debug_assert!(from < SCHEMA_VERSION);
    // No migration steps exist yet — there is no schema before v1.
    Err(StoreError::Corrupt {
        file: SETTINGS_FILE,
        reason: format!(
            "no migration path from version {from} (raw: {} bytes)",
            raw.len()
        ),
    })
}

/// Load `settings.toml` from `config_dir`. Missing file → defaults.
/// Unparseable/unmigratable file → backed up, defaults. Newer version →
/// best-effort recognized fields, marked **not writable** (docs/06 §4).
pub fn load_settings(config_dir: &Path) -> Result<LoadOutcome<Settings>, StoreError> {
    let Some(raw) = read_file(config_dir, SETTINGS_FILE)? else {
        return Ok(LoadOutcome::fresh(Settings::default()));
    };

    let version = declared_version_toml(&raw);
    if let Some(found) = version {
        if found > SCHEMA_VERSION {
            // Old build, newer file: load what we recognize, never overwrite.
            let settings = toml::from_str::<RawSettings>(&raw)
                .map(|r| coerce(r).0)
                .unwrap_or_default();
            return Ok(LoadOutcome {
                value: settings,
                writable: false,
                warning: Some(format!(
                    "{SETTINGS_FILE} was written by a newer Squash (schema v{found}); settings are read-only"
                )),
            });
        }
        if found < SCHEMA_VERSION {
            match migrate_settings(&raw, found) {
                Ok(s) => return Ok(LoadOutcome::fresh(s)),
                Err(_) => {
                    backup_file(config_dir, SETTINGS_FILE, &format!("v{found}"));
                    return Ok(LoadOutcome {
                        value: Settings::default(),
                        writable: true,
                        warning: Some(format!(
                            "{SETTINGS_FILE} v{found} could not be migrated; reset to defaults (backup kept)"
                        )),
                    });
                }
            }
        }
    }

    match toml::from_str::<RawSettings>(&raw) {
        Ok(raw) => {
            let (settings, warnings) = coerce(raw);
            Ok(LoadOutcome {
                value: settings,
                writable: true,
                warning: if warnings.is_empty() {
                    None
                } else {
                    Some(warnings.join("; "))
                },
            })
        }
        Err(err) => {
            backup_file(config_dir, SETTINGS_FILE, "corrupt");
            Ok(LoadOutcome {
                value: Settings::default(),
                writable: true,
                warning: Some(format!(
                    "{SETTINGS_FILE} was corrupt and reset to defaults ({err})"
                )),
            })
        }
    }
}

/// Coerce a leniently-parsed settings file into a valid [`Settings`],
/// applying the docs/06 §2 "unknown → default + warn" rules field by field.
fn coerce(raw: RawSettings) -> (Settings, Vec<String>) {
    let mut warnings = Vec::new();
    let mut warn = |msg: String| warnings.push(msg);
    let defaults = Settings::default();

    let language = match raw.language.as_deref() {
        None => defaults.language,
        Some("en") => Language::En,
        Some("ar") => Language::Ar,
        Some(other) => {
            warn(format!("unknown language {other:?}, using en"));
            Language::En
        }
    };
    let theme = match raw.theme.as_deref() {
        None => defaults.theme,
        Some("system") => Theme::System,
        Some("light") => Theme::Light,
        Some("dark") => Theme::Dark,
        Some(other) => {
            warn(format!("unknown theme {other:?}, using system"));
            Theme::System
        }
    };
    let default_preset = match raw.default_preset.as_deref() {
        Some(id) if Preset::from_id(id).is_some() || id.starts_with("user:") => id.to_string(),
        Some(other) => {
            warn(format!("unknown preset {other:?}, using builtin:balanced"));
            defaults.default_preset
        }
        None => defaults.default_preset,
    };
    let default_format = match raw
        .default_format
        .as_deref()
        .and_then(|f| f.parse::<crate::format::Format>().ok())
    {
        Some(f) if f.can_create() => f.name().to_string(),
        Some(f) => {
            warn(format!(
                "format {} is not create-capable, using zip",
                f.name()
            ));
            defaults.default_format
        }
        None => {
            if let Some(other) = raw.default_format.as_deref() {
                warn(format!("unknown format {other:?}, using zip"));
            }
            defaults.default_format
        }
    };
    let extract = match raw.extract {
        None => defaults.extract,
        Some(raw_extract) => ExtractSettings {
            dest_policy: match raw_extract.dest_policy.as_deref() {
                None => defaults.extract.dest_policy,
                Some("same_folder") => DestPolicy::SameFolder,
                Some("ask") => DestPolicy::Ask,
                Some(other) => {
                    warn(format!("unknown dest_policy {other:?}, using same_folder"));
                    DestPolicy::SameFolder
                }
            },
            loose_files_policy: match raw_extract.loose_files_policy.as_deref() {
                None => defaults.extract.loose_files_policy,
                Some("new_folder") => LooseFilesPolicy::NewFolder,
                Some("here") => LooseFilesPolicy::Here,
                Some(other) => {
                    warn(format!(
                        "unknown loose_files_policy {other:?}, using new_folder"
                    ));
                    LooseFilesPolicy::NewFolder
                }
            },
        },
    };

    let settings = Settings {
        version: SCHEMA_VERSION,
        language,
        theme,
        default_preset,
        default_format,
        extract,
        update_check_opt_in: raw.update_check_opt_in.unwrap_or(false),
        activation_counter_opt_in: raw.activation_counter_opt_in.unwrap_or(false),
        first_launch_done: raw.first_launch_done.unwrap_or(false),
        drop_zone_hint_dismissed: raw.drop_zone_hint_dismissed.unwrap_or(false),
        debug_logging: raw.debug_logging.unwrap_or(false),
        crash_reporting: raw.crash_reporting.unwrap_or(false),
    };
    (settings, warnings)
}

/// Persist `settings.toml` atomically (docs/06 §5). Unknown keys and user
/// comments in the existing file are preserved by overlaying known keys onto
/// the parsed document (docs/06 §2/§3). A newer-versioned file on disk is
/// never overwritten (docs/06 §4).
pub fn save_settings(config_dir: &Path, settings: &Settings) -> Result<(), StoreError> {
    if let Ok(Some(raw)) = read_file(config_dir, SETTINGS_FILE) {
        if let Some(found) = declared_version_toml(&raw) {
            if found > SCHEMA_VERSION {
                return Err(StoreError::TooNew {
                    file: SETTINGS_FILE,
                    found,
                });
            }
        }
    }

    // Serialize via the toml_edit overlay only: `toml::to_string` cannot emit
    // scalar keys after a table (`extract`), and the overlay keeps key order
    // stable anyway.
    let render = |s: &Settings| {
        let mut doc = toml_edit::DocumentMut::new();
        overlay(&mut doc, s);
        doc.to_string()
    };
    let text = match read_file(config_dir, SETTINGS_FILE)? {
        Some(raw) => match raw.parse::<toml_edit::DocumentMut>() {
            Ok(mut doc) => {
                overlay(&mut doc, settings);
                doc.to_string()
            }
            // Unparseable existing file: write fresh, keeping a backup.
            Err(_) => {
                backup_file(config_dir, SETTINGS_FILE, "corrupt");
                render(settings)
            }
        },
        None => render(settings),
    };
    write_file_atomic(config_dir, SETTINGS_FILE, text.as_bytes())?;
    Ok(())
}

/// Overwrite only the known keys on an existing document, preserving
/// comments and unknown keys (docs/06 §3: "toml_edit preserves user comments
/// on rewrite").
fn overlay(doc: &mut toml_edit::DocumentMut, s: &Settings) {
    use toml_edit::{value, Item, Table};
    doc["version"] = value(i64::from(SCHEMA_VERSION));
    doc["language"] = value(match s.language {
        Language::En => "en",
        Language::Ar => "ar",
    });
    doc["theme"] = value(match s.theme {
        Theme::System => "system",
        Theme::Light => "light",
        Theme::Dark => "dark",
    });
    doc["default_preset"] = value(s.default_preset.as_str());
    doc["default_format"] = value(s.default_format.as_str());
    if !matches!(doc.get("extract"), Some(Item::Table(_))) {
        doc["extract"] = Item::Table(Table::new());
    }
    doc["extract"]["dest_policy"] = value(match s.extract.dest_policy {
        DestPolicy::SameFolder => "same_folder",
        DestPolicy::Ask => "ask",
    });
    doc["extract"]["loose_files_policy"] = value(match s.extract.loose_files_policy {
        LooseFilesPolicy::NewFolder => "new_folder",
        LooseFilesPolicy::Here => "here",
    });
    doc["update_check_opt_in"] = value(s.update_check_opt_in);
    doc["activation_counter_opt_in"] = value(s.activation_counter_opt_in);
    doc["first_launch_done"] = value(s.first_launch_done);
    doc["drop_zone_hint_dismissed"] = value(s.drop_zone_hint_dismissed);
    doc["debug_logging"] = value(s.debug_logging);
    doc["crash_reporting"] = value(s.crash_reporting);
}

#[cfg(test)]
mod io_tests {
    use super::*;

    fn save_and_reload(dir: &Path, s: &Settings) -> LoadOutcome<Settings> {
        save_settings(dir, s).unwrap();
        load_settings(dir).unwrap()
    }

    #[test]
    fn missing_file_yields_writable_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = load_settings(tmp.path()).unwrap();
        assert_eq!(outcome.value, Settings::default());
        assert!(outcome.writable);
        assert!(outcome.warning.is_none());
        assert!(!tmp.path().join(SETTINGS_FILE).exists());
    }

    #[test]
    fn roundtrip_preserves_values() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Settings {
            language: Language::Ar,
            theme: Theme::Dark,
            default_preset: "builtin:max".into(),
            default_format: "tar.zst".into(),
            extract: ExtractSettings {
                dest_policy: DestPolicy::Ask,
                loose_files_policy: LooseFilesPolicy::Here,
            },
            update_check_opt_in: true,
            activation_counter_opt_in: true,
            first_launch_done: true,
            drop_zone_hint_dismissed: true,
            debug_logging: true,
            crash_reporting: true,
            ..Settings::default()
        };
        let outcome = save_and_reload(tmp.path(), &s);
        assert_eq!(outcome.value, s);
        assert!(outcome.warning.is_none());
        // Atomic write left no temp file behind (docs/06 §5).
        assert!(!tmp.path().join("settings.toml.tmp").exists());
    }

    #[test]
    fn rewrite_preserves_comments_and_unknown_keys() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(SETTINGS_FILE),
            "# Morgan's hand-tuned config\nversion = 1\ntheme = \"dark\"\nfuture_key = \"keep me\"\n",
        )
        .unwrap();
        let s = Settings {
            theme: Theme::Light,
            ..Settings::default()
        };
        save_settings(tmp.path(), &s).unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(SETTINGS_FILE)).unwrap();
        assert!(raw.contains("# Morgan's hand-tuned config"), "{raw}");
        assert!(raw.contains("future_key = \"keep me\""), "{raw}");
        assert!(raw.contains("theme = \"light\""), "{raw}");
    }

    #[test]
    fn unknown_values_coerce_to_defaults_with_warning() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(SETTINGS_FILE),
            "version = 1\nlanguage = \"fr\"\ntheme = \"neon\"\ndefault_format = \"rar\"\n",
        )
        .unwrap();
        let outcome = load_settings(tmp.path()).unwrap();
        assert_eq!(outcome.value.language, Language::En);
        assert_eq!(outcome.value.theme, Theme::System);
        assert_eq!(outcome.value.default_format, "zip");
        let warning = outcome.warning.expect("coercions produce a warning");
        assert!(warning.contains("fr"), "{warning}");
        assert!(warning.contains("neon"), "{warning}");
        assert!(warning.contains("rar"), "{warning}");
        assert!(outcome.writable);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_returned() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(SETTINGS_FILE), "[[[ not toml").unwrap();
        let outcome = load_settings(tmp.path()).unwrap();
        assert_eq!(outcome.value, Settings::default());
        assert!(outcome.writable);
        assert!(outcome.warning.is_some());
        assert!(!tmp.path().join(SETTINGS_FILE).exists());
        assert!(tmp.path().join("settings.toml.corrupt.bak").exists());
    }

    #[test]
    fn newer_version_loads_read_only_and_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let original = "version = 2\ntheme = \"dark\"\n";
        std::fs::write(tmp.path().join(SETTINGS_FILE), original).unwrap();
        let outcome = load_settings(tmp.path()).unwrap();
        assert!(!outcome.writable, "newer file must be read-only");
        assert!(outcome.warning.is_some());
        assert_eq!(outcome.value.theme, Theme::Dark, "recognized fields load");
        // Never overwrite (docs/06 §4).
        let err = save_settings(tmp.path(), &Settings::default()).unwrap_err();
        assert!(matches!(err, StoreError::TooNew { found: 2, .. }));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join(SETTINGS_FILE)).unwrap(),
            original
        );
    }

    #[test]
    fn older_version_is_backed_up_and_reset() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(SETTINGS_FILE),
            "version = 0\ntheme = \"dark\"\n",
        )
        .unwrap();
        let outcome = load_settings(tmp.path()).unwrap();
        assert_eq!(outcome.value, Settings::default());
        assert!(outcome.writable);
        assert!(outcome.warning.is_some());
        assert!(tmp.path().join("settings.toml.v0.bak").exists());
    }

    #[test]
    fn first_launch_flag_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        // Fresh installs default to "not done" so S7 shows (docs/03 F1).
        let outcome = load_settings(tmp.path()).unwrap();
        assert!(!outcome.value.first_launch_done);

        let done = Settings {
            first_launch_done: true,
            ..Settings::default()
        };
        let outcome = save_and_reload(tmp.path(), &done);
        assert!(outcome.value.first_launch_done);
        assert!(outcome.warning.is_none());
    }

    #[test]
    fn v1_file_without_newer_additive_keys_coerces_to_defaults() {
        // Additive keys (docs/06 §2 forward-compat): a v1 file written before
        // `drop_zone_hint_dismissed` existed must still load, defaulting the
        // missing key, and a save must then persist it.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(SETTINGS_FILE),
            "version = 1\nlanguage = \"ar\"\nfirst_launch_done = true\n",
        )
        .unwrap();
        let outcome = load_settings(tmp.path()).unwrap();
        assert!(!outcome.value.drop_zone_hint_dismissed);
        assert!(outcome.value.first_launch_done);
        assert!(outcome.warning.is_none(), "no coercion happened");

        let dismissed = Settings {
            drop_zone_hint_dismissed: true,
            ..outcome.value
        };
        let outcome = save_and_reload(tmp.path(), &dismissed);
        assert!(outcome.value.drop_zone_hint_dismissed);
        assert_eq!(
            outcome.value.language,
            Language::Ar,
            "existing keys survive"
        );
    }

    #[test]
    fn saved_file_carries_current_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        save_settings(tmp.path(), &Settings::default()).unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(SETTINGS_FILE)).unwrap();
        assert_eq!(declared_version_toml(&raw), Some(SCHEMA_VERSION));
    }
}
