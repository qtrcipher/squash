//! GUI host state (docs/06 §7): the live `Settings` + job table, queue and
//! history persistence, and the progress-event fan-out.
//!
//! Everything here is webview-free and unit-tested; `commands.rs` is the
//! thin Tauri layer on top. Progress reaches the frontend through the
//! [`ProgressSink`] trait — the real sink emits Tauri events, tests use a
//! recording sink.

use serde::Serialize;
use squash_core::job::{Job, Operation};
use squash_core::progress::{JobStats, ProgressEvent};
use squash_core::store::{
    self, HistoryOp, HistoryRecord, JobSource, JobStatus, Language, PersistedQueue, QueuedJob,
    Settings, SCHEMA_VERSION,
};
use squash_core::{Engine, JobHandle, SquashError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Tauri event name for job progress (typed payload: [`ProgressPayload`]).
pub const PROGRESS_EVENT: &str = "squash://job-progress";

/// Tauri event nudging the frontend to drain OS "open with" paths (docs/03
/// F6). The payload is empty on purpose: the frontend pulls the paths via
/// the `take_pending_open_paths` command, so a cold-start event that fires
/// before the webview subscribes is never lost and a warm-start event never
/// duplicates what the launch-time pull already drained.
pub const OPEN_PATHS_EVENT: &str = "squash://open-paths";

/// Compact history every N appends (docs/06 §5: "after every 50 appends").
const COMPACT_EVERY_APPENDS: u32 = 50;

/// The locale tag attached to crash reports (docs/06 §6).
fn language_tag(language: Language) -> &'static str {
    match language {
        Language::En => "en",
        Language::Ar => "ar",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryStatus {
    Queued,
    Running,
    Finished,
    Failed,
    Cancelled,
}

impl EntryStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Finished | Self::Failed | Self::Cancelled)
    }
}

/// The S4 row model (docs/03 S4, docs/06 §7 "one shape, two renderers").
/// Serialized camelCase for the TS frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEntryDto {
    /// UUID v4 string — stable across restarts for restored jobs.
    pub id: String,
    pub operation: Operation,
    /// Display name: output archive name (compress) or archive name (extract).
    pub label: String,
    pub inputs: Vec<String>,
    pub destination: String,
    pub format: String,
    /// Preset id, e.g. `builtin:balanced` (meaningless for extract jobs).
    pub preset: String,
    pub status: EntryStatus,
    pub total_bytes_estimate: Option<u64>,
    pub bytes_done: u64,
    pub entries_done: u64,
    pub in_bytes: Option<u64>,
    pub out_bytes: Option<u64>,
    pub duration_ms: Option<u64>,
    /// Stable `SquashError` code — the frontend re-localizes (docs/06 §2).
    pub error_code: Option<String>,
    /// RFC 3339 UTC, set at submission.
    pub started_at: String,
}

impl JobEntryDto {
    fn new(id: &str, job: &Job) -> Self {
        let label = match job.operation {
            Operation::Compress => job.destination.clone(),
            // Extraction destination is a directory; the archive name is the
            // meaningful label.
            Operation::Extract => job.inputs.first().cloned().unwrap_or_default(),
        };
        let label = label
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| label.to_string_lossy().into_owned());
        Self {
            id: id.to_string(),
            operation: job.operation,
            label,
            inputs: job
                .inputs
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            destination: job.destination.to_string_lossy().into_owned(),
            format: job.format.name().to_string(),
            preset: job.preset.id().to_string(),
            status: EntryStatus::Queued,
            total_bytes_estimate: None,
            bytes_done: 0,
            entries_done: 0,
            in_bytes: None,
            out_bytes: None,
            duration_ms: None,
            error_code: None,
            started_at: store::now_rfc3339(),
        }
    }
}

/// Typed progress payload for the `squash://job-progress` event — mirrors
/// the core [`ProgressEvent`] with the job id folded in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProgressPayload {
    #[serde(rename_all = "camelCase")]
    Started {
        id: String,
        total_bytes_estimate: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Advanced {
        id: String,
        bytes_done: u64,
        entries_done: u64,
        current_path: String,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        id: String,
        in_bytes: u64,
        out_bytes: u64,
        duration_ms: u64,
    },
    #[serde(rename_all = "camelCase")]
    Failed { id: String, error_code: String },
}

/// Where progress goes. The production impl emits Tauri events; tests record.
pub trait ProgressSink: Send + Sync {
    fn emit(&self, payload: &ProgressPayload);
}

pub struct TauriSink(pub tauri::AppHandle);

impl ProgressSink for TauriSink {
    fn emit(&self, payload: &ProgressPayload) {
        use tauri::Emitter;
        let _ = self.0.emit(PROGRESS_EVENT, payload);
    }
}

struct JobSlot {
    spec: Job,
    handle: JobHandle,
    entry: JobEntryDto,
}

pub struct AppState {
    pub dirs: store::StoreDirs,
    engine: Engine,
    slots: Mutex<HashMap<String, JobSlot>>,
    order: Mutex<Vec<String>>,
    settings: Mutex<Settings>,
    settings_writable: AtomicBool,
    settings_warning: Mutex<Option<String>>,
    queue_writable: AtomicBool,
    /// Queue as loaded at startup, waiting for [`AppState::restore_queue`].
    pending_restore: Mutex<Option<PersistedQueue>>,
    /// Paths handed over by the OS (docs/03 F6: argv, `RunEvent::Opened`,
    /// second-instance launch), waiting for the frontend to drain them.
    pending_open: Mutex<Vec<String>>,
    /// Crash-reporting client guard (docs/06 §6): `Some` only while a
    /// consented, DSN-carrying build has a live Sentry client. Kept so the
    /// client flushes on exit; `None` means zero crash-reporting code runs.
    crash_guard: Mutex<Option<squash_core::crash::ClientInitGuard>>,
    appends_since_compact: AtomicU32,
}

impl AppState {
    pub fn new(dirs: store::StoreDirs) -> Self {
        Self::with_engine(dirs, Engine::new())
    }

    /// Constructor with an injected engine — plain dependency injection so
    /// tests can pass a deterministically gated engine (see
    /// `squash_core::engine::JobStartGate`).
    pub fn with_engine(dirs: store::StoreDirs, engine: Engine) -> Self {
        let settings_outcome = store::load_settings(&dirs.config_dir).unwrap_or_else(|err| {
            eprintln!("squash: settings load failed ({err}); using defaults");
            store::LoadOutcome::fresh(Settings::default())
        });
        let (pending_restore, queue_writable) = match store::load_queue(&dirs.data_dir) {
            Ok(outcome) => {
                if let Some(warning) = &outcome.warning {
                    eprintln!("squash: {warning}");
                }
                (Some(outcome.value), outcome.writable)
            }
            Err(err) => {
                eprintln!("squash: queue load failed ({err}); starting empty");
                (Some(PersistedQueue::default()), true)
            }
        };
        // Retention is enforced at launch (docs/06 §5).
        if let Err(err) = store::enforce_retention_now(&dirs.data_dir) {
            eprintln!("squash: history compaction failed ({err})");
        }
        // Verbose debug log (docs/06 §3 "Debug log"): the sink is installed
        // at launch; the persisted S6 toggle decides whether it writes.
        crate::logging::init();
        if settings_outcome.value.debug_logging {
            crate::logging::enable(&dirs.log_dir);
        }
        // Opt-in crash reporting (docs/06 §6): consent off (the default) or
        // a DSN-less build → `None`, and no Sentry client ever exists.
        let crash_guard = squash_core::crash::init(
            settings_outcome.value.crash_reporting,
            "gui",
            Some(language_tag(settings_outcome.value.language)),
        );
        Self {
            dirs,
            engine,
            slots: Mutex::new(HashMap::new()),
            order: Mutex::new(Vec::new()),
            settings: Mutex::new(settings_outcome.value),
            settings_writable: AtomicBool::new(settings_outcome.writable),
            settings_warning: Mutex::new(settings_outcome.warning),
            queue_writable: AtomicBool::new(queue_writable),
            pending_restore: Mutex::new(pending_restore),
            pending_open: Mutex::new(Vec::new()),
            crash_guard: Mutex::new(crash_guard),
            appends_since_compact: AtomicU32::new(0),
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    // --- settings ---------------------------------------------------------

    pub fn settings_snapshot(&self) -> (Settings, bool, Option<String>) {
        (
            self.settings.lock().expect("settings lock").clone(),
            self.settings_writable.load(Ordering::Relaxed),
            self.settings_warning.lock().expect("settings lock").clone(),
        )
    }

    pub fn set_settings(&self, settings: Settings) -> Result<(), store::StoreError> {
        settings
            .validate()
            .map_err(|reason| store::StoreError::Corrupt {
                file: store::SETTINGS_FILE,
                reason,
            })?;
        if !self.settings_writable.load(Ordering::Relaxed) {
            return Err(store::StoreError::TooNew {
                file: store::SETTINGS_FILE,
                found: SCHEMA_VERSION + 1,
            });
        }
        store::save_settings(&self.dirs.config_dir, &settings)?;
        let debug_on = settings.debug_logging;
        let crash_on = settings.crash_reporting;
        let language = settings.language;
        let mut guard = self.settings.lock().expect("settings lock");
        let toggled = guard.debug_logging != debug_on;
        let crash_toggled = guard.crash_reporting != crash_on;
        *guard = settings;
        drop(guard);
        // The S6 verbose toggle takes effect immediately (docs/06 §3).
        if toggled {
            if debug_on {
                crate::logging::enable(&self.dirs.log_dir);
            } else {
                crate::logging::disable();
            }
        }
        // The crash-reporting toggle takes effect immediately too (docs/06
        // §6): on → initialize the client (DSN-less builds stay a no-op);
        // off → unbind it, so no report can leave from this point on.
        if crash_toggled {
            if crash_on {
                let mut slot = self.crash_guard.lock().expect("crash lock");
                if slot.is_none() {
                    *slot = squash_core::crash::init(true, "gui", Some(language_tag(language)));
                } else {
                    squash_core::crash::set_consent(true);
                }
            } else {
                squash_core::crash::shutdown();
                *self.crash_guard.lock().expect("crash lock") = None;
            }
        }
        Ok(())
    }

    // --- jobs -------------------------------------------------------------

    /// Submit a job under a fresh id. Returns the id.
    pub fn submit(&self, job: Job) -> String {
        self.submit_with_id(store::new_id(), job)
    }

    /// Submit under a specific id — used by queue restore so persisted uuids
    /// stay stable across restarts (docs/06 §2).
    pub fn submit_with_id(&self, id: String, job: Job) -> String {
        let handle = self.engine.submit(job.clone());
        let entry = JobEntryDto::new(&id, &job);
        self.slots.lock().expect("slots lock").insert(
            id.clone(),
            JobSlot {
                spec: job,
                handle,
                entry,
            },
        );
        self.order.lock().expect("order lock").push(id.clone());
        self.persist_queue();
        id
    }

    pub fn cancel(&self, id: &str) -> bool {
        let slots = self.slots.lock().expect("slots lock");
        match slots.get(id) {
            Some(slot) if !slot.entry.status.is_terminal() => {
                slot.handle.cancel();
                true
            }
            _ => false,
        }
    }

    /// Remove a terminal job from the list (S4 "Dismiss").
    pub fn dismiss(&self, id: &str) -> bool {
        let mut slots = self.slots.lock().expect("slots lock");
        match slots.get(id) {
            Some(slot) if slot.entry.status.is_terminal() => {
                slots.remove(id);
                drop(slots);
                self.order.lock().expect("order lock").retain(|j| j != id);
                true
            }
            _ => false,
        }
    }

    /// Re-submit a terminal job's stored spec (S4 "Retry"). Returns the new
    /// job id; the failed entry is replaced.
    pub fn retry(&self, id: &str) -> Option<String> {
        let spec = self.slots.lock().expect("slots lock").get(id)?.spec.clone();
        self.dismiss(id);
        Some(self.submit(spec))
    }

    pub fn snapshot(&self) -> Vec<JobEntryDto> {
        let slots = self.slots.lock().expect("slots lock");
        self.order
            .lock()
            .expect("order lock")
            .iter()
            .filter_map(|id| slots.get(id).map(|s| s.entry.clone()))
            .collect()
    }

    pub fn handle_for(&self, id: &str) -> Option<JobHandle> {
        self.slots
            .lock()
            .expect("slots lock")
            .get(id)
            .map(|s| s.handle.clone())
    }

    // --- OS "open with" handoff (docs/03 F6) ------------------------------

    /// Queue paths the OS passed in (argv / `RunEvent::Opened` / second
    /// instance) for the frontend to route to S2/S3.
    pub fn queue_open_paths(&self, paths: Vec<String>) {
        self.pending_open.lock().expect("open lock").extend(paths);
    }

    /// Drain the queued open paths. Pull-based so cold-start events that
    /// fired before the webview subscribed are still delivered.
    pub fn take_pending_open_paths(&self) -> Vec<String> {
        std::mem::take(&mut *self.pending_open.lock().expect("open lock"))
    }

    // --- restore (docs/06 §2 "Queued job") --------------------------------

    /// Restore the persisted queue: jobs whose inputs vanished are dropped
    /// into history as `cancelled`; the rest are resubmitted as queued.
    /// Returns the restored (resubmitted) job ids so the caller can attach
    /// progress forwarders.
    pub fn restore_queue(&self) -> Vec<String> {
        let Some(queue) = self.pending_restore.lock().expect("restore lock").take() else {
            return Vec::new();
        };
        if !self.queue_writable.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let (restorable, vanished) = store::partition_restorable(&queue);
        for dropped in &vanished {
            let record = self.history_record_for(&dropped.job, &dropped.id, JobStatus::Cancelled);
            if let Err(err) = self.append_history(&record) {
                eprintln!("squash: could not record dropped queue job ({err})");
            }
        }
        let mut ids = Vec::new();
        for queued in restorable {
            ids.push(self.submit_with_id(queued.id, queued.job));
        }
        ids
    }

    // --- progress ---------------------------------------------------------

    /// Fold one core [`ProgressEvent`] into the job entry and return the
    /// payload to broadcast. Terminal events also persist the queue and
    /// append the history record (docs/06 §2).
    pub fn apply_event(&self, id: &str, event: &ProgressEvent) -> ProgressPayload {
        enum Terminal {
            No,
            Finished(JobStats),
            Failed(SquashError),
        }
        let (payload, terminal) = {
            let mut slots = self.slots.lock().expect("slots lock");
            let Some(slot) = slots.get_mut(id) else {
                return ProgressPayload::Failed {
                    id: id.to_string(),
                    error_code: SquashError::Internal.code().to_string(),
                };
            };
            match event {
                ProgressEvent::Started {
                    total_bytes_estimate,
                } => {
                    slot.entry.status = EntryStatus::Running;
                    slot.entry.total_bytes_estimate = *total_bytes_estimate;
                    (
                        ProgressPayload::Started {
                            id: id.to_string(),
                            total_bytes_estimate: *total_bytes_estimate,
                        },
                        Terminal::No,
                    )
                }
                ProgressEvent::Advanced {
                    bytes_done,
                    entries_done,
                    current_path,
                } => {
                    slot.entry.bytes_done = *bytes_done;
                    slot.entry.entries_done = *entries_done;
                    (
                        ProgressPayload::Advanced {
                            id: id.to_string(),
                            bytes_done: *bytes_done,
                            entries_done: *entries_done,
                            current_path: current_path.to_string_lossy().into_owned(),
                        },
                        Terminal::No,
                    )
                }
                ProgressEvent::Finished { stats } => {
                    slot.entry.status = EntryStatus::Finished;
                    slot.entry.in_bytes = Some(stats.in_bytes);
                    slot.entry.out_bytes = Some(stats.out_bytes);
                    slot.entry.duration_ms =
                        Some(u64::try_from(stats.duration.as_millis()).unwrap_or(u64::MAX));
                    (
                        ProgressPayload::Finished {
                            id: id.to_string(),
                            in_bytes: stats.in_bytes,
                            out_bytes: stats.out_bytes,
                            duration_ms: u64::try_from(stats.duration.as_millis())
                                .unwrap_or(u64::MAX),
                        },
                        Terminal::Finished(*stats),
                    )
                }
                ProgressEvent::Failed { error } => {
                    let cancelled = *error == SquashError::Cancelled;
                    slot.entry.status = if cancelled {
                        EntryStatus::Cancelled
                    } else {
                        EntryStatus::Failed
                    };
                    slot.entry.error_code = Some(error.code().to_string());
                    (
                        ProgressPayload::Failed {
                            id: id.to_string(),
                            error_code: error.code().to_string(),
                        },
                        Terminal::Failed(error.clone()),
                    )
                }
            }
        };
        if !matches!(terminal, Terminal::No) {
            self.persist_queue();
            let (status, stats) = match &terminal {
                Terminal::Finished(s) => (JobStatus::Finished, Some(*s)),
                Terminal::Failed(e) => (
                    if *e == SquashError::Cancelled {
                        JobStatus::Cancelled
                    } else {
                        JobStatus::Failed
                    },
                    None,
                ),
                Terminal::No => unreachable!(),
            };
            self.record_terminal(id, status, stats);
        }
        payload
    }

    /// Append the history record for a job that just terminated.
    fn record_terminal(&self, id: &str, status: JobStatus, stats: Option<JobStats>) {
        let (spec, started_at, error_code) = {
            let slots = self.slots.lock().expect("slots lock");
            match slots.get(id) {
                Some(slot) => (
                    slot.spec.clone(),
                    slot.entry.started_at.clone(),
                    slot.entry.error_code.clone(),
                ),
                None => return,
            }
        };
        let mut record = self.history_record_for(&spec, id, status);
        record.started_at = started_at;
        record.error_code = error_code;
        if let Some(stats) = stats {
            record.in_bytes = Some(stats.in_bytes);
            record.out_bytes = Some(stats.out_bytes);
            record.duration_ms =
                Some(u64::try_from(stats.duration.as_millis()).unwrap_or(u64::MAX));
        }
        if let Err(err) = self.append_history(&record) {
            eprintln!("squash: history append failed ({err})");
        }
    }

    fn history_record_for(&self, job: &Job, id: &str, status: JobStatus) -> HistoryRecord {
        HistoryRecord {
            version: SCHEMA_VERSION,
            id: id.to_string(),
            op: match job.operation {
                Operation::Compress => HistoryOp::Compress,
                Operation::Extract => HistoryOp::Extract,
            },
            inputs: job
                .inputs
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            output: Some(job.destination.to_string_lossy().into_owned()),
            format: job.format.name().to_string(),
            preset: job.preset.id().to_string(),
            status,
            error_code: (status != JobStatus::Finished).then(|| {
                if status == JobStatus::Cancelled {
                    "cancelled".to_string()
                } else {
                    SquashError::Internal.code().to_string()
                }
            }),
            in_bytes: None,
            out_bytes: None,
            duration_ms: None,
            started_at: store::now_rfc3339(),
            source: JobSource::Gui,
        }
    }

    /// Append + periodic compaction (docs/06 §5: every 50 appends).
    fn append_history(&self, record: &HistoryRecord) -> Result<(), store::StoreError> {
        store::append_history(&self.dirs.data_dir, record)?;
        if self.appends_since_compact.fetch_add(1, Ordering::Relaxed) + 1 >= COMPACT_EVERY_APPENDS {
            self.appends_since_compact.store(0, Ordering::Relaxed);
            store::enforce_retention_now(&self.dirs.data_dir)?;
        }
        Ok(())
    }

    // --- queue persistence -------------------------------------------------

    /// Rewrite `queue.json` with the currently unfinished jobs (docs/06 §2:
    /// only queued/running jobs persist; running restores as queued).
    pub fn persist_queue(&self) {
        if !self.queue_writable.load(Ordering::Relaxed) {
            return;
        }
        let slots = self.slots.lock().expect("slots lock");
        let order = self.order.lock().expect("order lock");
        let mut position = 0u32;
        let mut jobs = Vec::new();
        for id in order.iter() {
            let Some(slot) = slots.get(id) else { continue };
            if slot.entry.status.is_terminal() {
                continue;
            }
            jobs.push(QueuedJob {
                id: id.clone(),
                position,
                job: slot.spec.clone(),
            });
            position += 1;
        }
        drop(order);
        drop(slots);
        let queue = PersistedQueue {
            version: SCHEMA_VERSION,
            jobs,
        };
        if let Err(err) = store::save_queue(&self.dirs.data_dir, &queue) {
            eprintln!("squash: queue persist failed ({err})");
        }
    }
}

/// Read progress events off a job's stream until it terminates, folding them
/// into [`AppState`] and emitting each to the sink. One thread per job; the
/// stream closes after the terminal event (core guarantee).
pub fn spawn_forwarder(state: Arc<AppState>, sink: Arc<dyn ProgressSink>, id: String) {
    let Some(handle) = state.handle_for(&id) else {
        return;
    };
    std::thread::spawn(move || {
        while let Some(event) = handle.next_event() {
            let terminal = matches!(
                event,
                ProgressEvent::Finished { .. } | ProgressEvent::Failed { .. }
            );
            let payload = state.apply_event(&id, &event);
            sink.emit(&payload);
            if terminal {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use squash_core::format::Format;
    use squash_core::presets::Preset;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    fn test_dirs() -> (tempfile::TempDir, store::StoreDirs) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = store::StoreDirs {
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
            log_dir: tmp.path().join("data/logs"),
        };
        (tmp, dirs)
    }

    struct RecordingSink(StdMutex<Vec<ProgressPayload>>);
    impl ProgressSink for RecordingSink {
        fn emit(&self, payload: &ProgressPayload) {
            self.0.lock().unwrap().push(payload.clone());
        }
    }

    fn compress_job(src: PathBuf, dest: PathBuf) -> Job {
        Job::compress(vec![src], dest, Format::Zip, Preset::Balanced)
    }

    #[test]
    fn submit_tracks_job_and_persists_queue() {
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs.clone());
        let id = state.submit(compress_job(
            PathBuf::from("/definitely/not/here"),
            PathBuf::from("/tmp/out.zip"),
        ));
        let snapshot = state.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, id);
        assert_eq!(snapshot[0].status, EntryStatus::Queued);
        // Unfinished job persisted (docs/06 §2).
        let queue = store::load_queue(&dirs.data_dir).unwrap().value;
        assert_eq!(queue.jobs.len(), 1);
        assert_eq!(queue.jobs[0].id, id);
    }

    #[test]
    fn terminal_event_updates_entry_persists_history_and_clears_queue() {
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs.clone());
        let src = dirs.data_dir.join("..").join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        let dest = src.join("a.zip");
        let id = state.submit(compress_job(src, dest.clone()));

        // Drive the real engine event stream through apply_event.
        let handle = state.handle_for(&id).unwrap();
        while let Some(event) = handle.next_event() {
            state.apply_event(&id, &event);
        }

        let snapshot = state.snapshot();
        assert_eq!(snapshot[0].status, EntryStatus::Finished);
        assert!(snapshot[0].out_bytes.is_some());
        // Finished jobs leave the persisted queue…
        let queue = store::load_queue(&dirs.data_dir).unwrap().value;
        assert!(queue.jobs.is_empty());
        // …and land in history (docs/06 §2).
        let history = store::load_history(&dirs.data_dir).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, JobStatus::Finished);
        assert_eq!(history[0].source, JobSource::Gui);
        assert!(dest.exists());
    }

    #[test]
    fn failed_job_records_error_code_and_stays_listed() {
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs.clone());
        let id = state.submit(compress_job(
            PathBuf::from("/definitely/not/here"),
            PathBuf::from("/tmp/out.zip"),
        ));
        let handle = state.handle_for(&id).unwrap();
        while let Some(event) = handle.next_event() {
            state.apply_event(&id, &event);
        }
        let snapshot = state.snapshot();
        assert_eq!(snapshot[0].status, EntryStatus::Failed);
        assert!(snapshot[0].error_code.is_some());
        let history = store::load_history(&dirs.data_dir).unwrap();
        assert_eq!(history[0].status, JobStatus::Failed);
    }

    #[test]
    fn cancel_marks_entry_cancelled() {
        let (_tmp, dirs) = test_dirs();
        // Deterministic, no timing: the held start gate keeps both jobs
        // queued, so the cancel lands before the target job can start —
        // cancellation of a *running* job is cooperative and timing-dependent.
        let gate = squash_core::engine::JobStartGate::new();
        let state = AppState::with_engine(dirs.clone(), Engine::new_with_start_gate(gate.clone()));
        let src = dirs.data_dir.join("..").join("cancel-src");
        std::fs::create_dir_all(&src).unwrap();
        let busy = state.submit(compress_job(src.clone(), src.join("busy.zip")));
        let id = state.submit(compress_job(
            PathBuf::from("/definitely/not/here"),
            PathBuf::from("/tmp/out.zip"),
        ));
        assert!(state.cancel(&id));
        gate.release();
        for job_id in [&busy, &id] {
            let handle = state.handle_for(job_id).unwrap();
            while let Some(event) = handle.next_event() {
                state.apply_event(job_id, &event);
            }
        }
        let snapshot = state.snapshot();
        let entry = snapshot.iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.status, EntryStatus::Cancelled);
        assert_eq!(entry.error_code.as_deref(), Some("cancelled"));
        let history = store::load_history(&dirs.data_dir).unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|r| r.status == JobStatus::Cancelled));
        // Cancelling twice is a no-op.
        assert!(!state.cancel(&id));
    }

    #[test]
    fn dismiss_removes_terminal_only_and_retry_resubmits() {
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs.clone());
        let id = state.submit(compress_job(
            PathBuf::from("/definitely/not/here"),
            PathBuf::from("/tmp/out.zip"),
        ));
        // Not terminal yet → dismiss refused.
        assert!(!state.dismiss(&id));
        let handle = state.handle_for(&id).unwrap();
        while let Some(event) = handle.next_event() {
            state.apply_event(&id, &event);
        }
        let new_id = state.retry(&id).expect("terminal job retries");
        assert_ne!(new_id, id);
        assert_eq!(state.snapshot().len(), 1, "retry replaces the entry");
        assert_eq!(state.snapshot()[0].id, new_id);
        let handle = state.handle_for(&new_id).unwrap();
        while let Some(event) = handle.next_event() {
            state.apply_event(&new_id, &event);
        }
        assert!(state.dismiss(&new_id));
        assert!(state.snapshot().is_empty());
    }

    #[test]
    fn restore_drops_vanished_inputs_to_history_as_cancelled() {
        let (_tmp, dirs) = test_dirs();
        let existing = dirs.data_dir.join("real.txt");
        std::fs::create_dir_all(&dirs.data_dir).unwrap();
        std::fs::write(&existing, b"x").unwrap();
        let queue = PersistedQueue {
            version: SCHEMA_VERSION,
            jobs: vec![
                QueuedJob {
                    id: "keep-me".into(),
                    position: 0,
                    job: compress_job(existing, dirs.data_dir.join("keep.zip")),
                },
                QueuedJob {
                    id: "drop-me".into(),
                    position: 1,
                    job: compress_job(
                        dirs.data_dir.join("gone.txt"),
                        dirs.data_dir.join("drop.zip"),
                    ),
                },
            ],
        };
        store::save_queue(&dirs.data_dir, &queue).unwrap();

        let state = AppState::new(dirs.clone());
        let restored = state.restore_queue();
        assert_eq!(restored, vec!["keep-me".to_string()]);
        // Vanished input → history as cancelled (docs/06 §2).
        let history = store::load_history(&dirs.data_dir).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "drop-me");
        assert_eq!(history[0].status, JobStatus::Cancelled);
        // Drain the restored job so the worker thread finishes.
        let handle = state.handle_for("keep-me").unwrap();
        while handle.next_event().is_some() {}
    }

    #[test]
    fn restore_is_skipped_for_newer_queue_files() {
        let (_tmp, dirs) = test_dirs();
        std::fs::create_dir_all(&dirs.data_dir).unwrap();
        std::fs::write(
            dirs.data_dir.join(store::QUEUE_FILE),
            r#"{"version": 9, "jobs": []}"#,
        )
        .unwrap();
        let state = AppState::new(dirs.clone());
        assert!(state.restore_queue().is_empty());
        // Never overwritten (docs/06 §4).
        state.persist_queue();
        let raw = std::fs::read_to_string(dirs.data_dir.join(store::QUEUE_FILE)).unwrap();
        assert!(raw.contains("\"version\": 9"));
    }

    #[test]
    fn forwarder_emits_every_event_in_order() {
        let (_tmp, dirs) = test_dirs();
        let state = Arc::new(AppState::new(dirs.clone()));
        let src = dirs.data_dir.join("..").join("fw-src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        let id = state.submit(compress_job(src.clone(), src.join("a.zip")));
        let sink = Arc::new(RecordingSink(StdMutex::new(Vec::new())));
        spawn_forwarder(Arc::clone(&state), sink.clone(), id.clone());
        // Wait for the terminal state to land.
        for _ in 0..100 {
            if state.snapshot()[0].status.is_terminal() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        for _ in 0..100 {
            let events = sink.0.lock().unwrap();
            if matches!(events.last(), Some(ProgressPayload::Finished { .. })) {
                break;
            }
            drop(events);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let events = sink.0.lock().unwrap();
        assert!(matches!(
            events.first(),
            Some(ProgressPayload::Started { .. })
        ));
        assert!(matches!(
            events.last(),
            Some(ProgressPayload::Finished { .. })
        ));
        assert!(events.iter().all(|p| match p {
            ProgressPayload::Started { id: e, .. }
            | ProgressPayload::Advanced { id: e, .. }
            | ProgressPayload::Finished { id: e, .. }
            | ProgressPayload::Failed { id: e, .. } => *e == id,
        }));
    }

    #[test]
    fn open_paths_queue_drains_once() {
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs);
        assert!(state.take_pending_open_paths().is_empty());
        state.queue_open_paths(vec!["/tmp/a.zip".to_string()]);
        state.queue_open_paths(vec!["/tmp/b.tar.gz".to_string(), "/tmp/c".to_string()]);
        assert_eq!(
            state.take_pending_open_paths(),
            vec!["/tmp/a.zip", "/tmp/b.tar.gz", "/tmp/c"]
        );
        // Drained: a second pull returns nothing (no duplicate sheets).
        assert!(state.take_pending_open_paths().is_empty());
    }

    #[test]
    fn debug_logging_toggle_persists_and_writes_log_file() {
        // Serialize with logging.rs's global-logger test.
        let _guard = crate::logging::TEST_LOCK.lock().unwrap();
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs.clone());
        let (mut settings, _, _) = state.settings_snapshot();
        assert!(!settings.debug_logging);

        settings.debug_logging = true;
        state.set_settings(settings).unwrap();
        // Persisted through the settings discipline (docs/06 §2)…
        let reloaded = store::load_settings(&dirs.config_dir).unwrap().value;
        assert!(reloaded.debug_logging);
        // …and the host-side log file now exists in the log dir (docs/06 §3).
        let log_file = dirs.log_dir.join(crate::logging::LOG_FILE);
        assert!(log_file.exists());
        let contents = std::fs::read_to_string(&log_file).unwrap();
        assert!(contents.contains("verbose logging started"), "{contents}");

        let mut off = state.settings_snapshot().0;
        off.debug_logging = false;
        state.set_settings(off).unwrap();
        let reloaded = store::load_settings(&dirs.config_dir).unwrap().value;
        assert!(!reloaded.debug_logging);
        crate::logging::disable();
    }

    #[test]
    fn crash_reporting_toggle_gates_the_client() {
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs.clone());
        // Default off (docs/06 §6): the consent gate is unset and this
        // DSN-less dev build initialized no client — zero network possible.
        assert!(!squash_core::crash::consent_given());
        assert!(!squash_core::crash::available());

        let (mut settings, _, _) = state.settings_snapshot();
        settings.crash_reporting = true;
        state.set_settings(settings).unwrap();
        // Persisted through the settings discipline (docs/06 §2).
        assert!(
            store::load_settings(&dirs.config_dir)
                .unwrap()
                .value
                .crash_reporting
        );
        // Without a baked-in DSN the gate stays off even when the user
        // opted in — the toggle explains "not available in this build".
        assert!(!squash_core::crash::consent_given());

        let mut off = state.settings_snapshot().0;
        off.crash_reporting = false;
        state.set_settings(off).unwrap();
        assert!(!squash_core::crash::consent_given());
        assert!(
            !store::load_settings(&dirs.config_dir)
                .unwrap()
                .value
                .crash_reporting
        );
    }

    #[test]
    fn settings_roundtrip_and_read_only_guard() {
        let (_tmp, dirs) = test_dirs();
        let state = AppState::new(dirs.clone());
        let (settings, writable, warning) = state.settings_snapshot();
        assert!(writable);
        assert!(warning.is_none());
        let mut updated = settings.clone();
        updated.theme = store::Theme::Dark;
        state.set_settings(updated.clone()).unwrap();
        assert_eq!(state.settings_snapshot().0, updated);
        // Persisted on disk.
        let reloaded = store::load_settings(&dirs.config_dir).unwrap().value;
        assert_eq!(reloaded, updated);

        // A newer-version file on disk makes saves fail closed (docs/06 §4).
        let (_tmp2, dirs2) = test_dirs();
        std::fs::create_dir_all(&dirs2.config_dir).unwrap();
        std::fs::write(
            dirs2.config_dir.join(store::SETTINGS_FILE),
            "version = 3\ntheme = \"dark\"\n",
        )
        .unwrap();
        let state2 = AppState::new(dirs2);
        let (s2, writable2, warning2) = state2.settings_snapshot();
        assert!(!writable2);
        assert!(warning2.is_some());
        assert!(state2.set_settings(s2).is_err());
    }
}
