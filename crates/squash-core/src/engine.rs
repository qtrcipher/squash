//! Engine facade (docs/05 §3).
//!
//! `Engine::submit(job) -> JobHandle` is the single entry point for work,
//! shared by GUI and CLI.
//!
//! **Concurrency policy (documented choice):** jobs run **one at a time, in
//! submission order**, on a single worker thread owned by the engine. That
//! keeps progress attribution and partial-output cleanup trivially correct;
//! parallelism belongs to the batch layer above (the GUI queue submits many
//! jobs), not to the engine. Cancellation is cooperative: handlers check the
//! handle's flag between entries.

use crate::error::SquashError;
use crate::format::{FormatRegistry, HandlerContext};
use crate::job::{Job, JobId, Operation};
use crate::progress::{JobStats, ProgressEvent};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;

pub struct Engine {
    registry: Arc<FormatRegistry>,
    next_job: AtomicU64,
    queue: mpsc::Sender<WorkItem>,
}

/// Test-only start gate: while held, the worker thread blocks *before*
/// dequeuing each job, so every submitted job stays queued until
/// [`JobStartGate::release`]. Exists so cancellation tests can deterministically
/// cancel a queued job instead of racing the worker; **not public API**
/// (`#[doc(hidden)]`). Once released, behavior is identical to an ungated
/// engine and the gate is never consulted again in practice (the wait is a
/// no-op check per dequeue).
#[doc(hidden)]
#[derive(Clone)]
pub struct JobStartGate(Arc<(Mutex<bool>, Condvar)>);

impl JobStartGate {
    /// Create a held gate — no job starts until [`JobStartGate::release`].
    pub fn new() -> Self {
        Self(Arc::new((Mutex::new(false), Condvar::new())))
    }

    /// Let the worker run: drains every queued job, then runs normally.
    pub fn release(&self) {
        let (lock, cvar) = &*self.0;
        *lock.lock().expect("gate lock") = true;
        cvar.notify_all();
    }

    fn wait(&self) {
        let (lock, cvar) = &*self.0;
        let mut open = lock.lock().expect("gate lock");
        while !*open {
            open = cvar.wait(open).expect("gate lock");
        }
    }
}

impl Default for JobStartGate {
    fn default() -> Self {
        Self::new()
    }
}

struct WorkItem {
    job: Job,
    cancelled: Arc<AtomicBool>,
    events: mpsc::Sender<ProgressEvent>,
}

impl Engine {
    pub fn new() -> Self {
        let registry = Arc::new(FormatRegistry::new());
        let (tx, rx) = mpsc::channel::<WorkItem>();
        let worker_registry = Arc::clone(&registry);
        thread::spawn(move || {
            for item in rx {
                run_job(&worker_registry, item);
            }
        });
        Self {
            registry,
            next_job: AtomicU64::new(1),
            queue: tx,
        }
    }

    /// Engine whose worker blocks on `gate` before dequeuing each job.
    /// Test-only companion of [`Engine::new`] — see [`JobStartGate`] for why
    /// this exists and why it is `#[doc(hidden)]`.
    #[doc(hidden)]
    pub fn new_with_start_gate(gate: JobStartGate) -> Self {
        let registry = Arc::new(FormatRegistry::new());
        let (tx, rx) = mpsc::channel::<WorkItem>();
        let worker_registry = Arc::clone(&registry);
        thread::spawn(move || {
            loop {
                // Wait BEFORE dequeuing: a held gate keeps every job queued.
                gate.wait();
                match rx.recv() {
                    Ok(item) => run_job(&worker_registry, item),
                    Err(_) => break,
                }
            }
        });
        Self {
            registry,
            next_job: AtomicU64::new(1),
            queue: tx,
        }
    }

    pub fn registry(&self) -> &FormatRegistry {
        &self.registry
    }

    /// Queue the job and hand back a handle immediately. Progress events are
    /// read from the handle; cancellation is via [`JobHandle::cancel`].
    pub fn submit(&self, job: Job) -> JobHandle {
        let id = JobId(self.next_job.fetch_add(1, Ordering::Relaxed));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        // The queue only closes if the engine itself is gone, which cannot
        // happen while `&self` is borrowed — best-effort send regardless.
        let _ = self.queue.send(WorkItem {
            job,
            cancelled: Arc::clone(&cancelled),
            events: tx,
        });
        JobHandle {
            id,
            cancelled,
            events: Arc::new(Mutex::new(rx)),
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

fn run_job(registry: &FormatRegistry, item: WorkItem) {
    let send = |event: ProgressEvent| {
        let _ = item.events.send(event);
    };

    log::debug!(
        "job start: {} [{}] → {} (format {}, preset {})",
        match item.job.operation {
            Operation::Compress => "compress",
            Operation::Extract => "extract",
        },
        item.job
            .inputs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        item.job.destination.display(),
        item.job.format.name(),
        item.job.preset.id(),
    );

    if item.cancelled.load(Ordering::Relaxed) {
        log::debug!("job cancelled before it started");
        send(ProgressEvent::Failed {
            error: SquashError::Cancelled,
        });
        return;
    }

    let Some(handler) = registry.handler_for(item.job.format) else {
        log::debug!(
            "no handler registered for format {}",
            item.job.format.name()
        );
        send(ProgressEvent::Failed {
            error: SquashError::UnsupportedFormat,
        });
        return;
    };
    let capable = match item.job.operation {
        Operation::Compress => item.job.format.can_create() && handler.can_create(),
        Operation::Extract => handler.can_extract(),
    };
    if !capable {
        log::debug!(
            "format {} cannot {} — rejecting",
            item.job.format.name(),
            match item.job.operation {
                Operation::Compress => "create",
                Operation::Extract => "extract",
            }
        );
        send(ProgressEvent::Failed {
            error: SquashError::UnsupportedFormat,
        });
        return;
    }

    let reporter = |event: ProgressEvent| {
        let _ = item.events.send(event);
    };
    let ctx = HandlerContext::new(&item.cancelled, &reporter);

    // Compress estimates total input bytes; extraction stays indeterminate
    // (the uncompressed size of a tar stream needs a full pass to know).
    let total_bytes_estimate = match item.job.operation {
        Operation::Compress => crate::formats::inputs_total_bytes(&item.job.inputs),
        Operation::Extract => None,
    };
    send(ProgressEvent::Started {
        total_bytes_estimate,
    });

    let result = match item.job.operation {
        Operation::Compress => handler.create(&item.job, &ctx),
        Operation::Extract => match item.job.inputs.first() {
            Some(archive) => handler.extract(archive, &item.job.destination, &ctx),
            None => Err(SquashError::Internal),
        },
    };

    match result {
        Ok(stats) => {
            log::debug!(
                "job finished: {} → {} bytes in {:?}",
                stats.in_bytes,
                stats.out_bytes,
                stats.duration,
            );
            send(ProgressEvent::Finished { stats });
        }
        Err(error) => {
            log::debug!("job failed: {}", error.code());
            // docs/03 F2: partial compress output is deleted automatically.
            if item.job.operation == Operation::Compress {
                let _ = std::fs::remove_file(&item.job.destination);
            }
            send(ProgressEvent::Failed { error });
        }
    }
}

/// Handle for a submitted job: cancellation + the progress event stream.
///
/// The stream closes after `Finished`/`Failed` is delivered. Cloned handles
/// share one stream — whichever clone reads an event consumes it (the CLI
/// uses a single reader; the GUI should too).
#[derive(Clone)]
pub struct JobHandle {
    id: JobId,
    cancelled: Arc<AtomicBool>,
    events: Arc<Mutex<mpsc::Receiver<ProgressEvent>>>,
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

    /// Block until the next event. `None` once the stream has closed
    /// (the job terminated and every event was delivered).
    pub fn next_event(&self) -> Option<ProgressEvent> {
        self.events.lock().expect("progress lock").recv().ok()
    }

    /// Non-blocking event read.
    pub fn try_next_event(&self) -> Option<ProgressEvent> {
        self.events.lock().expect("progress lock").try_recv().ok()
    }

    /// Drain the stream until the job terminates.
    pub fn wait(&self) -> Result<JobStats, SquashError> {
        loop {
            match self.next_event() {
                Some(ProgressEvent::Finished { stats }) => return Ok(stats),
                Some(ProgressEvent::Failed { error }) => return Err(error),
                Some(_) => {}
                // Stream closed without a terminal event: should not happen.
                None => return Err(SquashError::Internal),
            }
        }
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
                vec![PathBuf::from("/definitely/not/here")],
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

    #[test]
    fn failed_job_reports_failed_event() {
        let engine = Engine::new();
        let handle = engine.submit(Job::compress(
            vec![PathBuf::from("/definitely/not/here")],
            PathBuf::from("out.zip"),
            Format::Zip,
            Preset::Balanced,
        ));
        assert!(handle.wait().is_err());
    }

    #[test]
    fn missing_input_fails_the_job() {
        let engine = Engine::new();
        // Every registered format gets as far as the handler; a missing
        // input surfaces as an io-class failure, not a panic or a hang.
        let handle = engine.submit(Job::extract(
            vec![PathBuf::from("/definitely/not/here.gz")],
            PathBuf::from("out"),
            Format::Gz,
        ));
        assert!(handle.wait().is_err());
    }

    #[test]
    fn create_unsupported_capability_is_rejected() {
        let engine = Engine::new();
        // tar.bz2 is extract-only (docs/05 §4).
        let handle = engine.submit(Job::compress(
            vec![PathBuf::from("in")],
            PathBuf::from("out.tar.bz2"),
            Format::TarBz2,
            Preset::Balanced,
        ));
        assert_eq!(handle.wait(), Err(SquashError::UnsupportedFormat));
    }

    #[test]
    fn cancelled_queued_job_reports_cancelled() {
        // Deterministic, no timing: the held start gate keeps both jobs
        // queued, so the cancel lands before the target job can start.
        let gate = JobStartGate::new();
        let engine = Engine::new_with_start_gate(gate.clone());
        let tmp = tempfile::tempdir().unwrap();
        let busy = engine.submit(Job::compress(
            vec![tmp.path().join("src")],
            tmp.path().join("busy.zip"),
            Format::Zip,
            Preset::Fast,
        ));
        let queued = engine.submit(Job::extract(
            vec![tmp.path().join("busy.zip")],
            tmp.path().join("out"),
            Format::Zip,
        ));
        queued.cancel();
        gate.release();
        let _ = busy.wait();
        assert_eq!(queued.wait(), Err(SquashError::Cancelled));
    }

    #[test]
    fn progress_stream_shape_for_compress() {
        let engine = Engine::new();
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        let handle = engine.submit(Job::compress(
            vec![src],
            tmp.path().join("a.zip"),
            Format::Zip,
            Preset::Balanced,
        ));
        let mut events = Vec::new();
        while let Some(event) = handle.next_event() {
            events.push(event);
        }
        assert!(matches!(
            events.first(),
            Some(ProgressEvent::Started { .. })
        ));
        assert!(matches!(
            events.last(),
            Some(ProgressEvent::Finished { .. })
        ));
        assert!(events
            .iter()
            .any(|e| matches!(e, ProgressEvent::Advanced { .. })));
    }
}
