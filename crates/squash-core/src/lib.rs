//! Squash shared compression core.
//!
//! One core, two thin shells (CLI + Tauri GUI) — see `docs/05-architecture.md`.
//! Phase 1: this crate defines the public API *surface* (types, traits, error
//! taxonomy, preset table, store schemas). No compression is implemented yet;
//! format handlers land in Phase 2 behind [`format::FormatHandler`].

pub mod engine;
pub mod error;
pub mod format;
pub mod job;
pub mod presets;
pub mod progress;
pub mod store;

pub use engine::{Engine, JobHandle};
pub use error::SquashError;
pub use format::{Format, FormatHandler, FormatRegistry};
pub use job::{Job, JobId, JobOptions, Operation};
pub use presets::{Preset, PresetParams};
pub use progress::{JobStats, ProgressEvent};
