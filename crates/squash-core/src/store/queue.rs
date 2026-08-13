//! `queue.json` schema (docs/06 §2 "Queued job").
//!
//! Exact serde mirror of the core [`Job`] type — single source of truth, zero
//! mapping layer. Holds only *unfinished* jobs; at restore, entries whose
//! inputs vanished are dropped into history as `cancelled` (persistence
//! phase, docs/06 §2).

use crate::job::Job;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedJob {
    /// UUID v4 string.
    pub id: String,
    pub position: u32,
    pub job: Job,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedQueue {
    pub version: u32,
    pub jobs: Vec<QueuedJob>,
}

impl Default for PersistedQueue {
    fn default() -> Self {
        Self {
            version: super::SCHEMA_VERSION,
            jobs: Vec::new(),
        }
    }
}

impl PersistedQueue {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != super::SCHEMA_VERSION {
            return Err(format!("unsupported queue version {}", self.version));
        }
        Ok(())
    }
}
