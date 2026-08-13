//! Engine facade (docs/05 §3).
//!
//! `Engine::submit(job) -> JobHandle` is the single entry point for work,
//! shared by GUI and CLI. Phase 1 shell: `submit` allocates a [`JobId`] and
//! returns a cancellable handle; no work is scheduled yet — the real engine
//! (formats, safety layer, progress stream) is Phase 2.

use crate::format::FormatRegistry;
use crate::job::{Job, JobId};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub struct Engine {
    registry: FormatRegistry,
    next_job: AtomicU64,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            registry: FormatRegistry::new(),
            next_job: AtomicU64::new(1),
        }
    }

    pub fn registry(&self) -> &FormatRegistry {
        &self.registry
    }

    /// Phase 1: accepts the job, hands back a handle. Returns the handle
    /// immediately; execution lands with the Phase 2 engine.
    pub fn submit(&self, _job: Job) -> JobHandle {
        JobHandle {
            id: JobId(self.next_job.fetch_add(1, Ordering::Relaxed)),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// Cancellation handle for a submitted job.
#[derive(Clone)]
pub struct JobHandle {
    id: JobId,
    cancelled: Arc<AtomicBool>,
}

impl JobHandle {
    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::presets::Preset;
    use std::path::PathBuf;

    #[test]
    fn submit_allocates_unique_ids_and_cancellation_works() {
        let engine = Engine::new();
        let job = || {
            Job::compress(
                vec![PathBuf::from("in")],
                PathBuf::from("out.zip"),
                Format::Zip,
                Preset::Balanced,
            )
        };
        let a = engine.submit(job());
        let b = engine.submit(job());
        assert_ne!(a.id(), b.id());
        assert!(!a.is_cancelled());
        a.cancel();
        assert!(a.is_cancelled());
        assert!(!b.is_cancelled());
    }
}
