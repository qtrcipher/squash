//! Local-only store (docs/06): schemas, validation, and file I/O.
//!
//! Layout per docs/06 §3: `settings.toml` + `presets.toml` in the config dir,
//! `history.jsonl` + `queue.json` in the data dir. All functions are pure —
//! they take the directory explicitly (docs/06 §7), never global state, so
//! shells pass `ProjectDirs` paths and tests pass tempdirs.
//!
//! Conventions (docs/06 §2): timestamps are RFC 3339 UTC strings; paths are
//! plain strings, never canonicalized; every schema carries `version = 1`.
//! Whole files are written temp-then-rename; history lines append under an
//! advisory lock (docs/06 §5).

pub mod history;
pub mod presets;
pub mod queue;
pub mod settings;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use history::{
    append_history, enforce_retention, enforce_retention_now, load_history, HistoryOp,
    HistoryRecord, JobSource, JobStatus,
};
pub use presets::{UserPreset, MAX_USER_PRESETS};
pub use queue::{load_queue, partition_restorable, save_queue, PersistedQueue, QueuedJob};
pub use settings::{
    load_settings, save_settings, DestPolicy, ExtractSettings, Language, LooseFilesPolicy,
    Settings, Theme,
};

/// Current schema version for every store file (docs/06 §4).
pub const SCHEMA_VERSION: u32 = 1;

/// Store file names — one file per entity, so a corrupt history can never
/// take settings down with it (docs/06 §3).
pub const SETTINGS_FILE: &str = "settings.toml";
pub const QUEUE_FILE: &str = "queue.json";
pub const HISTORY_FILE: &str = "history.jsonl";

/// Errors from the persistence layer.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("corrupt {file}: {reason}")]
    Corrupt { file: &'static str, reason: String },
    /// docs/06 §4: an old build never overwrites a newer-version file.
    #[error("{file} has newer schema version {found} (supported: {SCHEMA_VERSION})")]
    TooNew { file: &'static str, found: u32 },
}

/// Outcome of loading a store file: the value plus whether the file may be
/// written back (docs/06 §4 — newer-version files are read-only) and an
/// optional user-facing warning key/detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadOutcome<T> {
    pub value: T,
    pub writable: bool,
    pub warning: Option<String>,
}

impl<T> LoadOutcome<T> {
    pub fn fresh(value: T) -> Self {
        Self {
            value,
            writable: true,
            warning: None,
        }
    }
}

/// The two directories the whole data model lives in (docs/06 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDirs {
    /// `settings.toml`, `presets.toml`.
    pub config_dir: PathBuf,
    /// `history.jsonl`, `queue.json`.
    pub data_dir: PathBuf,
}

impl StoreDirs {
    /// Resolve via the `directories` crate (docs/06 §1): qualifier
    /// `dev.squash`, app `Squash` → macOS `~/Library/Application Support/
    /// dev.squash.Squash`, Linux `~/.config/squash`, etc.
    pub fn resolve() -> Option<Self> {
        let dirs = directories::ProjectDirs::from("dev.squash", "", "Squash")?;
        Some(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
        })
    }
}

/// Current time as an RFC 3339 UTC string (docs/06 §2).
pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC 3339 formatting is infallible")
}

/// Parse an RFC 3339 UTC timestamp back (retention checks).
pub(crate) fn parse_rfc3339(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

/// New UUID v4 string for history records and queue entries (docs/06 §2).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Write `contents` to `dir/name` atomically (docs/06 §5): write
/// `<name>.tmp` in the same directory, fsync, rename over the target. A
/// leftover `.tmp` at load is ignored by readers.
pub(crate) fn write_file_atomic(dir: &Path, name: &str, contents: &[u8]) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    let target = dir.join(name);
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Back up a corrupt/unmigratable file to `<name>.<tag>.bak` (docs/06 §4).
/// Best-effort: a failed backup never masks the original problem.
pub(crate) fn backup_file(dir: &Path, name: &str, tag: &str) {
    let from = dir.join(name);
    let to = dir.join(format!("{name}.{tag}.bak"));
    let _ = fs::rename(&from, &to);
}

/// Read a whole store file. `Ok(None)` when absent (fresh install).
pub(crate) fn read_file(dir: &Path, name: &str) -> io::Result<Option<String>> {
    match fs::read_to_string(dir.join(name)) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Schema version declared by a file that failed normal handling, used for
/// the never-overwrite-newer check before writes (docs/06 §4). `None` when
/// the file is absent, unparseable, or carries no version — in those cases
/// the write path is free to proceed (corrupt files get `.bak`ed).
pub(crate) fn declared_version_toml(raw: &str) -> Option<u32> {
    raw.parse::<toml_edit::DocumentMut>()
        .ok()?
        .get("version")?
        .as_integer()
        .and_then(|v| u32::try_from(v).ok())
}

pub(crate) fn declared_version_json(raw: &str) -> Option<u32> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()?
        .get("version")?
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_dirs_resolve_on_this_platform() {
        let dirs = StoreDirs::resolve().expect("ProjectDirs resolves on desktop OSes");
        assert!(dirs.config_dir.is_absolute());
        assert!(dirs.data_dir.is_absolute());
    }

    #[test]
    fn atomic_write_roundtrip_leaves_no_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        write_file_atomic(tmp.path(), "settings.toml", b"version = 1\n").unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("settings.toml")).unwrap(),
            "version = 1\n"
        );
        assert!(!tmp.path().join("settings.toml.tmp").exists());
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        write_file_atomic(tmp.path(), "queue.json", b"{}").unwrap();
        write_file_atomic(tmp.path(), "queue.json", b"{\"version\":1}").unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("queue.json")).unwrap(),
            "{\"version\":1}"
        );
    }

    #[test]
    fn atomic_write_creates_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a/b");
        write_file_atomic(&nested, "x.txt", b"hi").unwrap();
        assert_eq!(fs::read_to_string(nested.join("x.txt")).unwrap(), "hi");
    }

    #[test]
    fn now_rfc3339_roundtrips_through_parser() {
        let stamp = now_rfc3339();
        assert!(parse_rfc3339(&stamp).is_some(), "bad stamp: {stamp}");
        assert!(parse_rfc3339("not a date").is_none());
    }

    #[test]
    fn declared_versions_detected() {
        assert_eq!(declared_version_toml("version = 3\nx = 1"), Some(3));
        assert_eq!(declared_version_toml("garbage [[["), None);
        assert_eq!(declared_version_json(r#"{"version": 2}"#), Some(2));
        assert_eq!(declared_version_json("nope"), None);
    }

    #[test]
    fn load_outcome_fresh_is_writable() {
        let outcome = LoadOutcome::fresh(42);
        assert!(outcome.writable);
        assert_eq!(outcome.value, 42);
        assert!(outcome.warning.is_none());
    }
}
