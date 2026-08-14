//! Squash shared compression core.
//!
//! One core, two thin shells (CLI + Tauri GUI) — see `docs/05-architecture.md`.
//! Phase 2: the engine executes jobs on a worker thread; zip, 7z, the tar
//! family and the gz/xz/zst single-file codecs are implemented behind
//! [`format::FormatHandler`]; every extraction path passes through the
//! [`safety`] zip-slip layer.

pub mod engine;
pub mod error;
pub mod format;
pub mod formats;
pub mod job;
pub mod layout;
pub mod presets;
pub mod progress;
pub mod safety;
pub mod store;

pub use engine::{Engine, JobHandle};
pub use error::SquashError;
pub use format::{Format, FormatHandler, FormatRegistry, HandlerContext};
pub use job::{Job, JobId, JobOptions, Operation};
pub use presets::{Preset, PresetParams};
pub use progress::{JobStats, ProgressEvent};

/// Whether this build includes RAR extraction (docs/05 §4/§7) — reported in
/// the verbose-log header so support can tell builds apart.
pub const FEATURE_RAR: bool = cfg!(feature = "rar");
