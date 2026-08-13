//! Progress events (docs/05 §3).
//!
//! Every job emits a `ProgressEvent` stream. Tauri channels and the CLI
//! progress bar both adapt this one type — it is deliberately not serde;
//! the CLI's `--json` projection is a separate, documented schema (Phase 2).

use crate::error::SquashError;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    Started {
        total_bytes_estimate: Option<u64>,
    },
    Advanced {
        bytes_done: u64,
        entries_done: u64,
        current_path: PathBuf,
    },
    Finished {
        stats: JobStats,
    },
    Failed {
        error: SquashError,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobStats {
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub duration: Duration,
}
