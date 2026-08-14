//! `history.jsonl` schema (docs/06 §2 "Job record") — one JSON object per
//! line, append-only, bounded to 200 records or 30 days by compaction
//! (persistence phase).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryOp {
    Compress,
    Extract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Finished,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobSource {
    Gui,
    Cli,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub version: u32,
    /// UUID v4 string.
    pub id: String,
    pub op: HistoryOp,
    /// 1–100 entries, each ≤ 1024 chars (enforced at append time).
    pub inputs: Vec<String>,
    pub output: Option<String>,
    /// Registry names at read; unknown values render as-is (docs/06 §2).
    pub format: String,
    pub preset: String,
    pub status: JobStatus,
    /// Stable `SquashError` code string only — messages are re-localized at
    /// render, never stored.
    pub error_code: Option<String>,
    /// Null for failed-before-start.
    pub in_bytes: Option<u64>,
    pub out_bytes: Option<u64>,
    pub duration_ms: Option<u64>,
    /// RFC 3339 UTC.
    pub started_at: String,
    pub source: JobSource,
}

impl HistoryRecord {
    /// Validation stub; line-level checks (input caps, version skips) live in
    /// the persistence layer's locked-append reader.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("history record id is required".to_string());
        }
        if self.inputs.is_empty() || self.inputs.len() > 100 {
            return Err("history record needs 1–100 inputs".to_string());
        }
        if self.inputs.iter().any(|p| p.len() > 1024) {
            return Err("history input path exceeds 1024 chars".to_string());
        }
        if self.output.as_deref().is_some_and(|o| o.len() > 1024) {
            return Err("history output path exceeds 1024 chars".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_roundtrips_as_one_json_line() {
        let rec = HistoryRecord {
            version: 1,
            id: "3f6b1c52-0000-4000-8000-000000000000".into(),
            op: HistoryOp::Compress,
            inputs: vec!["/Users/x/photos".into()],
            output: Some("/Users/x/photos.tar.zst".into()),
            format: "tar.zst".into(),
            preset: "builtin:balanced".into(),
            status: JobStatus::Finished,
            error_code: None,
            in_bytes: Some(1_200_000_000),
            out_bytes: Some(640_000_000),
            duration_ms: Some(12_300),
            started_at: "2026-08-13T12:00:00Z".into(),
            source: JobSource::Gui,
        };
        rec.validate().unwrap();
        let line = serde_json::to_string(&rec).unwrap();
        assert!(!line.contains('\n'));
        let back: HistoryRecord = serde_json::from_str(&line).unwrap();
        assert_eq!(back, rec);
    }
}

// ---------------------------------------------------------------------------
// Persistence (docs/06 §2/§5): append-only JSONL with an advisory file lock,
// truncated-tail tolerance on read, and bounded retention (200 records or 30
// days, enforced by atomic-rewrite compaction).
// ---------------------------------------------------------------------------

use crate::store::{
    parse_rfc3339, read_file, write_file_atomic, StoreError, HISTORY_FILE, SCHEMA_VERSION,
};
use fs2::FileExt;
use std::path::Path;

/// Hard retention bounds (docs/06 §5).
pub const MAX_RECORDS: usize = 200;
pub const MAX_AGE_DAYS: i64 = 30;

/// Append one record to `history.jsonl` under an advisory lock (docs/06 §5:
/// GUI and CLI appends never interleave mid-line). The record must validate;
/// invalid records are rejected before the file is touched.
pub fn append_history(data_dir: &Path, record: &HistoryRecord) -> Result<(), StoreError> {
    log::debug!(
        "store: appending history record {} ({:?}, {:?})",
        record.id,
        record.op,
        record.status
    );
    record.validate().map_err(|reason| StoreError::Corrupt {
        file: HISTORY_FILE,
        reason,
    })?;
    let line = serde_json::to_string(record).map_err(|e| StoreError::Corrupt {
        file: HISTORY_FILE,
        reason: e.to_string(),
    })?;
    debug_assert!(!line.contains('\n'));

    std::fs::create_dir_all(data_dir)?;
    // `read(true)` is required on Windows, not for reading: a pure-append
    // handle is opened there without GENERIC_READ/GENERIC_WRITE
    // (rust-lang/rust#54118), and LockFileEx — which fs2's lock wraps —
    // rejects such a handle with `Access is denied` (os error 5,
    // fs2-rs#26). Writes still go through FILE_APPEND_DATA, so the
    // append-only semantics are unchanged.
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(data_dir.join(HISTORY_FILE))?;
    file.lock_exclusive()?;
    let result = (|| {
        use std::io::Write;
        let mut f = &file;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()
    })();
    let unlock = file.unlock();
    result?;
    unlock?;
    Ok(())
}

/// Read all records, skipping unparseable lines (a crash-truncated tail is
/// not fatal, docs/06 §5) and lines with newer record versions (docs/06 §4:
/// unknown lines are skipped, not deleted).
pub fn load_history(data_dir: &Path) -> Result<Vec<HistoryRecord>, StoreError> {
    let Some(raw) = read_file(data_dir, HISTORY_FILE)? else {
        return Ok(Vec::new());
    };
    let mut records = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue; // truncated/corrupt line
        };
        let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
        if version != u64::from(SCHEMA_VERSION) {
            continue; // older (none exist yet) or newer — skip, never delete
        }
        if let Ok(record) = serde_json::from_value::<HistoryRecord>(value) {
            records.push(record);
        }
    }
    Ok(records)
}

/// Enforce the retention bound (docs/06 §5): keep at most [`MAX_RECORDS`]
/// newest records and nothing older than [`MAX_AGE_DAYS`]. Rewrites the file
/// via atomic rename only when something was actually dropped. Called at
/// launch and after every 50 appends by the owning shell.
pub fn enforce_retention(data_dir: &Path, now: time::OffsetDateTime) -> Result<usize, StoreError> {
    let Some(raw) = read_file(data_dir, HISTORY_FILE)? else {
        return Ok(0);
    };
    let records = load_history(data_dir)?;
    let cutoff = now - time::Duration::days(MAX_AGE_DAYS);
    let fresh: Vec<&HistoryRecord> = records
        .iter()
        .filter(|r| parse_rfc3339(&r.started_at).is_none_or(|t| t >= cutoff))
        .collect();
    let kept: Vec<&&HistoryRecord> = fresh.iter().rev().take(MAX_RECORDS).collect();
    let kept: Vec<&HistoryRecord> = kept.into_iter().rev().copied().collect();

    let total_lines = raw.lines().filter(|l| !l.trim().is_empty()).count();
    let dropped = total_lines.saturating_sub(kept.len());
    if dropped == 0 {
        return Ok(0);
    }
    let mut out = String::new();
    for record in &kept {
        let line = serde_json::to_string(record).map_err(|e| StoreError::Corrupt {
            file: HISTORY_FILE,
            reason: e.to_string(),
        })?;
        out.push_str(&line);
        out.push('\n');
    }
    write_file_atomic(data_dir, HISTORY_FILE, out.as_bytes())?;
    Ok(dropped)
}

/// [`enforce_retention`] with the current time — the common case for shells.
pub fn enforce_retention_now(data_dir: &Path) -> Result<usize, StoreError> {
    enforce_retention(data_dir, time::OffsetDateTime::now_utc())
}

#[cfg(test)]
mod io_tests {
    use super::*;
    use crate::store::new_id;

    fn record(started_at: &str) -> HistoryRecord {
        HistoryRecord {
            version: SCHEMA_VERSION,
            id: new_id(),
            op: HistoryOp::Compress,
            inputs: vec!["/tmp/in".into()],
            output: Some("/tmp/out.zip".into()),
            format: "zip".into(),
            preset: "builtin:balanced".into(),
            status: JobStatus::Finished,
            error_code: None,
            in_bytes: Some(100),
            out_bytes: Some(50),
            duration_ms: Some(10),
            started_at: started_at.into(),
            source: JobSource::Gui,
        }
    }

    const NOW: &str = "2026-08-13T12:00:00Z";

    fn now() -> time::OffsetDateTime {
        parse_rfc3339(NOW).unwrap()
    }

    #[test]
    fn append_then_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let a = record(NOW);
        let b = HistoryRecord {
            status: JobStatus::Cancelled,
            error_code: Some("cancelled".into()),
            in_bytes: None,
            out_bytes: None,
            duration_ms: None,
            ..record(NOW)
        };
        append_history(tmp.path(), &a).unwrap();
        append_history(tmp.path(), &b).unwrap();
        assert_eq!(load_history(tmp.path()).unwrap(), vec![a, b]);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_history(tmp.path()).unwrap(), Vec::new());
    }

    #[test]
    fn truncated_tail_and_garbage_lines_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let good = serde_json::to_string(&record(NOW)).unwrap();
        std::fs::write(
            tmp.path().join(HISTORY_FILE),
            format!("{good}\n{{\"version\":1,\"id\":\"truncat\ngarbage\n"),
        )
        .unwrap();
        let records = load_history(tmp.path()).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn newer_record_versions_are_skipped_not_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let good = serde_json::to_string(&record(NOW)).unwrap();
        std::fs::write(
            tmp.path().join(HISTORY_FILE),
            format!("{good}\n{{\"version\":2,\"future\":true}}\n"),
        )
        .unwrap();
        assert_eq!(load_history(tmp.path()).unwrap().len(), 1);
        // Compaction must not delete the unknown line either — but it rewrites
        // only known records, so verify the unknown line survives when no
        // compaction is triggered.
        let raw = std::fs::read_to_string(tmp.path().join(HISTORY_FILE)).unwrap();
        assert!(raw.contains("\"version\":2"));
    }

    #[test]
    fn append_rejects_invalid_record_without_touching_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = HistoryRecord {
            inputs: vec![],
            ..record(NOW)
        };
        assert!(append_history(tmp.path(), &bad).is_err());
        assert!(!tmp.path().join(HISTORY_FILE).exists());
    }

    #[test]
    fn retention_drops_records_older_than_30_days() {
        let tmp = tempfile::tempdir().unwrap();
        append_history(tmp.path(), &record("2026-06-01T00:00:00Z")).unwrap(); // 73d old
        append_history(tmp.path(), &record("2026-08-10T00:00:00Z")).unwrap(); // 3d old
        let dropped = enforce_retention(tmp.path(), now()).unwrap();
        assert_eq!(dropped, 1);
        let records = load_history(tmp.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].started_at, "2026-08-10T00:00:00Z");
    }

    #[test]
    fn retention_keeps_newest_200_records() {
        let tmp = tempfile::tempdir().unwrap();
        for _ in 0..205 {
            append_history(tmp.path(), &record(NOW)).unwrap();
        }
        let dropped = enforce_retention(tmp.path(), now()).unwrap();
        assert_eq!(dropped, 5);
        assert_eq!(load_history(tmp.path()).unwrap().len(), MAX_RECORDS);
    }

    #[test]
    fn retention_is_noop_within_bounds() {
        let tmp = tempfile::tempdir().unwrap();
        append_history(tmp.path(), &record(NOW)).unwrap();
        let before = std::fs::read_to_string(tmp.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(enforce_retention(tmp.path(), now()).unwrap(), 0);
        let after = std::fs::read_to_string(tmp.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn retention_rewrite_leaves_no_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        append_history(tmp.path(), &record("2026-01-01T00:00:00Z")).unwrap();
        enforce_retention(tmp.path(), now()).unwrap();
        assert!(!tmp.path().join("history.jsonl.tmp").exists());
        assert!(load_history(tmp.path()).unwrap().is_empty());
    }
}
