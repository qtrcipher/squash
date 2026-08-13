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
