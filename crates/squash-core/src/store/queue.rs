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

// ---------------------------------------------------------------------------
// Persistence (docs/06 §2/§4/§5). JSON mirror of the core `Job` type;
// whole-file atomic writes; only *unfinished* jobs are ever written.
// ---------------------------------------------------------------------------

use crate::store::{
    backup_file, declared_version_json, read_file, write_file_atomic, LoadOutcome, StoreError,
    QUEUE_FILE, SCHEMA_VERSION,
};
use std::path::Path;

/// Load `queue.json` from `data_dir`. Missing/corrupt → empty queue (corrupt
/// files are backed up); newer version → empty and **not writable** so an old
/// build never clobbers a newer queue (docs/06 §4).
pub fn load_queue(data_dir: &Path) -> Result<LoadOutcome<PersistedQueue>, StoreError> {
    let Some(raw) = read_file(data_dir, QUEUE_FILE)? else {
        return Ok(LoadOutcome::fresh(PersistedQueue::default()));
    };

    if let Some(found) = declared_version_json(&raw) {
        if found > SCHEMA_VERSION {
            return Ok(LoadOutcome {
                value: PersistedQueue::default(),
                writable: false,
                warning: Some(format!(
                    "{QUEUE_FILE} was written by a newer Squash (schema v{found}); queue restore skipped"
                )),
            });
        }
        if found < SCHEMA_VERSION {
            // Forward-only chain has no steps yet (v1 is the first schema).
            backup_file(data_dir, QUEUE_FILE, &format!("v{found}"));
            return Ok(LoadOutcome {
                value: PersistedQueue::default(),
                writable: true,
                warning: Some(format!(
                    "{QUEUE_FILE} v{found} could not be migrated; starting with an empty queue (backup kept)"
                )),
            });
        }
    }

    match serde_json::from_str::<PersistedQueue>(&raw) {
        Ok(queue) => Ok(LoadOutcome::fresh(queue)),
        Err(err) => {
            backup_file(data_dir, QUEUE_FILE, "corrupt");
            Ok(LoadOutcome {
                value: PersistedQueue::default(),
                writable: true,
                warning: Some(format!("{QUEUE_FILE} was corrupt and ignored ({err})")),
            })
        }
    }
}

/// Persist `queue.json` atomically (docs/06 §5). A newer-versioned file on
/// disk is never overwritten (docs/06 §4).
pub fn save_queue(data_dir: &Path, queue: &PersistedQueue) -> Result<(), StoreError> {
    if let Ok(Some(raw)) = read_file(data_dir, QUEUE_FILE) {
        if let Some(found) = declared_version_json(&raw) {
            if found > SCHEMA_VERSION {
                return Err(StoreError::TooNew {
                    file: QUEUE_FILE,
                    found,
                });
            }
        }
    }
    let text = serde_json::to_string_pretty(queue).map_err(|e| StoreError::Corrupt {
        file: QUEUE_FILE,
        reason: e.to_string(),
    })?;
    write_file_atomic(data_dir, QUEUE_FILE, text.as_bytes())?;
    Ok(())
}

/// Split a restored queue into jobs whose inputs still exist (restorable,
/// resubmitted as queued) and jobs whose inputs vanished (dropped into
/// history as `cancelled`, docs/06 §2). Relative order is preserved.
pub fn partition_restorable(queue: &PersistedQueue) -> (Vec<QueuedJob>, Vec<QueuedJob>) {
    let mut jobs: Vec<&QueuedJob> = queue.jobs.iter().collect();
    jobs.sort_by_key(|j| j.position);
    let mut restorable = Vec::new();
    let mut vanished = Vec::new();
    for job in jobs {
        if job.job.inputs.iter().all(|p| p.exists()) {
            restorable.push(job.clone());
        } else {
            vanished.push(job.clone());
        }
    }
    (restorable, vanished)
}

#[cfg(test)]
mod io_tests {
    use super::*;
    use crate::format::Format;
    use crate::presets::Preset;
    use std::path::PathBuf;

    fn sample_job(inputs: Vec<PathBuf>) -> QueuedJob {
        QueuedJob {
            id: crate::store::new_id(),
            position: 0,
            job: Job::compress(
                inputs,
                PathBuf::from("out.zip"),
                Format::Zip,
                Preset::Balanced,
            ),
        }
    }

    #[test]
    fn missing_file_yields_empty_writable_queue() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = load_queue(tmp.path()).unwrap();
        assert_eq!(outcome.value, PersistedQueue::default());
        assert!(outcome.writable);
        assert!(outcome.warning.is_none());
    }

    #[test]
    fn roundtrip_preserves_jobs() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = PersistedQueue {
            version: SCHEMA_VERSION,
            jobs: vec![sample_job(vec![PathBuf::from("/tmp/in")])],
        };
        save_queue(tmp.path(), &queue).unwrap();
        let outcome = load_queue(tmp.path()).unwrap();
        assert_eq!(outcome.value, queue);
        assert!(!tmp.path().join("queue.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_backed_up_and_empty_returned() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(QUEUE_FILE), "{not json").unwrap();
        let outcome = load_queue(tmp.path()).unwrap();
        assert_eq!(outcome.value, PersistedQueue::default());
        assert!(outcome.writable);
        assert!(tmp.path().join("queue.json.corrupt.bak").exists());
    }

    #[test]
    fn newer_version_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let original = r#"{"version": 9, "jobs": [{"future": true}]}"#;
        std::fs::write(tmp.path().join(QUEUE_FILE), original).unwrap();
        let outcome = load_queue(tmp.path()).unwrap();
        assert!(!outcome.writable);
        assert!(outcome.value.jobs.is_empty(), "unknown jobs skipped");
        let err = save_queue(tmp.path(), &PersistedQueue::default()).unwrap_err();
        assert!(matches!(err, StoreError::TooNew { found: 9, .. }));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join(QUEUE_FILE)).unwrap(),
            original
        );
    }

    #[test]
    fn partition_drops_jobs_with_vanished_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let existing = tmp.path().join("here.txt");
        std::fs::write(&existing, b"x").unwrap();

        let mut keep = sample_job(vec![existing]);
        keep.position = 1;
        let mut drop = sample_job(vec![tmp.path().join("gone.txt")]);
        drop.position = 0;
        let queue = PersistedQueue {
            version: SCHEMA_VERSION,
            jobs: vec![keep.clone(), drop.clone()],
        };

        let (restorable, vanished) = partition_restorable(&queue);
        assert_eq!(restorable, vec![keep]);
        assert_eq!(vanished, vec![drop]);
    }

    #[test]
    fn partition_restores_in_position_order() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        let b = tmp.path().join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let mut first = sample_job(vec![a]);
        first.position = 2;
        let mut second = sample_job(vec![b]);
        second.position = 1;
        let queue = PersistedQueue {
            version: SCHEMA_VERSION,
            jobs: vec![first.clone(), second.clone()],
        };
        let (restorable, vanished) = partition_restorable(&queue);
        assert_eq!(restorable, vec![second, first]);
        assert!(vanished.is_empty());
    }
}
