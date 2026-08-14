//! Verbose debug log (docs/06 §3 "Debug log"): a `log`-crate sink writing a
//! rolling local file under `StoreDirs::log_dir`. Enabled by the S6
//! `debug_logging` toggle; disabled (and nothing is written) by default.
//!
//! The file may contain absolute paths — that is the point of a debug log.
//! It never leaves the device: the user reveals the folder from S6 and
//! chooses to attach the file to a GitHub issue.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Log file name inside the log dir.
pub const LOG_FILE: &str = "squash.log";
/// One previous generation is kept on rotation.
const ROTATED_FILE: &str = "squash.1.log";
/// Rotate at 1 MiB: plenty of detail for a bug report, never a disk problem.
const MAX_BYTES: u64 = 1024 * 1024;

/// Append-only rolling writer over `squash.log` (+ one rotated generation).
struct RollingFile {
    path: PathBuf,
    rotated: PathBuf,
    file: File,
    len: u64,
}

impl RollingFile {
    /// Open `<dir>/squash.log` for appending, rotating an oversized existing
    /// log first so a fresh session starts with the current state.
    fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(LOG_FILE);
        let rotated = dir.join(ROTATED_FILE);
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len >= MAX_BYTES {
            let _ = fs::rename(&path, &rotated);
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            rotated,
            file,
            len,
        })
    }

    fn write_line(&mut self, line: &str) {
        if self.len >= MAX_BYTES {
            // Best-effort rotation mid-session: on failure we keep appending
            // (a too-big log is a nuisance, not an error worth surfacing).
            if fs::rename(&self.path, &self.rotated).is_ok() {
                if let Ok(file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)
                {
                    self.file = file;
                    self.len = 0;
                }
            }
        }
        let bytes = line.len() as u64 + 1;
        if writeln!(self.file, "{line}").is_ok() {
            self.len += bytes;
        }
    }
}

/// The global sink: `None` = verbose logging off, records dropped.
pub struct GuiLogger(Mutex<Option<RollingFile>>);

impl log::Log for GuiLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut guard = self.0.lock().expect("logger lock");
        if let Some(file) = guard.as_mut() {
            file.write_line(&format!(
                "[{} {:5} {}] {}",
                squash_core::store::now_rfc3339(),
                record.level(),
                record.target(),
                record.args()
            ));
        }
    }

    fn flush(&self) {}
}

static LOGGER: GuiLogger = GuiLogger(Mutex::new(None));

/// Install the global sink. Idempotent: the `log` crate allows one global
/// logger, so repeat calls (tests) are no-ops. Max level is Debug — verbose
/// detail flows only while a file is attached.
pub fn init() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
}

/// Start writing the verbose log under `dir`, leading with the support
/// header (app version, OS, enabled features). Errors are reported to stderr
/// and never block the app — logging is a support feature, not critical I/O.
pub fn enable(dir: &Path) {
    match RollingFile::open(dir) {
        Ok(mut file) => {
            file.write_line(&format!(
                "=== squash {} ({} {}, rar {}) — verbose logging started {} ===",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
                if squash_core::FEATURE_RAR {
                    "on"
                } else {
                    "off"
                },
                squash_core::store::now_rfc3339(),
            ));
            *LOGGER.0.lock().expect("logger lock") = Some(file);
        }
        Err(err) => eprintln!("squash: could not start verbose logging ({err})"),
    }
}

/// Stop writing; later records are dropped until the next `enable`.
pub fn disable() {
    if let Some(mut file) = LOGGER.0.lock().expect("logger lock").take() {
        file.write_line(&format!(
            "=== verbose logging stopped {} ===",
            squash_core::store::now_rfc3339()
        ));
    }
}

/// The current log file path under `dir` (may not exist until enabled).
pub fn log_file_path(dir: &Path) -> PathBuf {
    dir.join(LOG_FILE)
}

/// The global logger is process-global: tests that enable it (here and in
/// `state.rs`) must serialize so one test's `enable` never steals another's
/// file mid-assertion.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_file_rotates_an_oversized_log_on_open() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(LOG_FILE), vec![b'x'; MAX_BYTES as usize]).unwrap();
        let mut file = RollingFile::open(tmp.path()).unwrap();
        assert!(tmp.path().join(ROTATED_FILE).exists(), "old log rotated");
        file.write_line("fresh line");
        drop(file);
        let current = std::fs::read_to_string(tmp.path().join(LOG_FILE)).unwrap();
        assert_eq!(current, "fresh line\n");
    }

    #[test]
    fn verbose_session_writes_header_records_and_stop_line() {
        // The one test that drives the global logger: enable → records land
        // in the tempdir log → disable stops writes.
        let _guard = TEST_LOCK.lock().unwrap();
        init();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("logs");
        enable(&dir);
        let path = log_file_path(&dir);
        assert!(path.exists(), "log file created on enable");
        log::debug!("engine did a thing");
        log::logger().flush();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("verbose logging started"), "{contents}");
        assert!(contents.contains(env!("CARGO_PKG_VERSION")), "{contents}");
        assert!(contents.contains(std::env::consts::OS), "{contents}");
        assert!(contents.contains("DEBUG"), "{contents}");
        assert!(contents.contains("engine did a thing"), "{contents}");

        disable();
        let after_disable = std::fs::read_to_string(&path).unwrap();
        assert!(after_disable.contains("verbose logging stopped"));
        log::debug!("this must not be written");
        log::logger().flush();
        let final_contents = std::fs::read_to_string(&path).unwrap();
        assert!(!final_contents.contains("this must not be written"));
    }
}
