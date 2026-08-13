//! Job model (docs/05 §3).
//!
//! Jobs are the *only* unit of work — the GUI batch queue and the CLI both
//! submit `Job`s to one `Engine::submit`. This type is also the exact serde
//! mirror persisted in `queue.json` (docs/06 §2), so its schema is stable.

use crate::format::Format;
use crate::presets::Preset;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Opaque job identifier, handed out by [`crate::Engine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Compress,
    Extract,
}

/// Per-job options. Phase 1: empty placeholder — overwrite policy,
/// loose-files handling and friends arrive with the Phase 2 engine.
/// Kept as a struct so `queue.json` entries already carry the field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOptions {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub operation: Operation,
    pub inputs: Vec<PathBuf>,
    /// Output archive (compress) or destination directory (extract).
    pub destination: PathBuf,
    pub format: Format,
    pub preset: Preset,
    #[serde(default)]
    pub options: JobOptions,
}

impl Job {
    pub fn compress(
        inputs: Vec<PathBuf>,
        destination: PathBuf,
        format: Format,
        preset: Preset,
    ) -> Self {
        Self {
            operation: Operation::Compress,
            inputs,
            destination,
            format,
            preset,
            options: JobOptions::default(),
        }
    }

    pub fn extract(inputs: Vec<PathBuf>, destination: PathBuf, format: Format) -> Self {
        Self {
            operation: Operation::Extract,
            inputs,
            destination,
            format,
            // Preset is meaningless for extraction; stored for schema parity.
            preset: Preset::default(),
            options: JobOptions::default(),
        }
    }
}
