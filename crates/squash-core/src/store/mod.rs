//! Local-only store types (docs/06).
//!
//! Phase 1 ships the **schemas and validation only** — no file I/O yet.
//! Layout per docs/06 §3: `settings.toml` + `presets.toml` in the config dir,
//! `history.jsonl` + `queue.json` in the data dir. Load/append/atomic-write
//! helpers (`load_settings`, `append_history`, …) arrive with persistence in
//! a later phase; they will be pure functions taking a directory, never
//! global state (docs/06 §7).
//!
//! Conventions (docs/06 §2): timestamps are RFC 3339 UTC strings; paths are
//! plain strings, never canonicalized; every schema carries `version = 1`.

pub mod history;
pub mod presets;
pub mod queue;
pub mod settings;

pub use history::{HistoryRecord, JobSource, JobStatus};
pub use presets::{UserPreset, MAX_USER_PRESETS};
pub use queue::{PersistedQueue, QueuedJob};
pub use settings::{DestPolicy, ExtractSettings, Language, LooseFilesPolicy, Settings, Theme};

/// Current schema version for every store file (docs/06 §4).
pub const SCHEMA_VERSION: u32 = 1;
