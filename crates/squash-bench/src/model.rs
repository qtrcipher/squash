//! Result data model: the `--json` schema, `benches/results.jsonl` rows and
//! `benches/baseline.json` all share these types. `BenchRun::VERSION` is the
//! schema version — bump on breaking changes.

use serde::{Deserialize, Serialize};

pub const RUN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineInfo {
    pub os: String,
    pub arch: String,
    pub cores: u32,
    /// CPU model string where the OS exposes one (best-effort).
    pub cpu: Option<String>,
    pub ram_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusInfo {
    pub seed: u64,
    pub scale: f64,
    pub total_bytes: u64,
    pub sets: Vec<String>,
}

/// One measured cell: tool × format × preset × corpus set. Durations are
/// medians over `reps` timed runs (after warmup, excluded).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchResult {
    /// `squash`, `7zz`, `gzip`, `zstd`, `xz`, …
    pub tool: String,
    /// Archive shape compared: `zip`, `7z`, `tar.gz`, `tar.zst`, `tar.xz`.
    pub format: String,
    pub preset: String,
    /// Codec level actually passed to the tool (honest level mapping).
    pub level: u8,
    pub set: String,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub compress_ms: u64,
    pub decompress_ms: u64,
    pub reps: u32,
}

impl BenchResult {
    /// out / in — lower is better.
    pub fn ratio(&self) -> f64 {
        if self.in_bytes == 0 {
            return 0.0;
        }
        self.out_bytes as f64 / self.in_bytes as f64
    }

    /// Input MiB (1024²) processed per second during compression.
    pub fn compress_mib_s(&self) -> f64 {
        mib_s(self.in_bytes, self.compress_ms)
    }

    /// Original-content MiB restored per second during decompression.
    pub fn decompress_mib_s(&self) -> f64 {
        mib_s(self.in_bytes, self.decompress_ms)
    }
}

fn mib_s(bytes: u64, ms: u64) -> f64 {
    if ms == 0 {
        return f64::INFINITY;
    }
    (bytes as f64 / 1_048_576.0) / (ms as f64 / 1000.0)
}

/// One benchmark run: machine + corpus context plus every measured cell.
/// This is the `--json` output and the `benches/baseline.json` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchRun {
    pub version: u32,
    /// Unix epoch seconds (no chrono dependency; see report for display).
    pub timestamp_epoch_s: u64,
    pub machine: MachineInfo,
    pub corpus: CorpusInfo,
    /// Human-readable lines for competitors that were not benchmarked
    /// (binary absent, GUI-only, license) — surfaced in reports, never
    /// silently dropped.
    pub skipped: Vec<String>,
    pub results: Vec<BenchResult>,
}

impl BenchRun {
    pub fn results_for(
        &self,
        tool: &str,
        format: &str,
        preset: &str,
        set: &str,
    ) -> Option<&BenchResult> {
        self.results
            .iter()
            .find(|r| r.tool == tool && r.format == format && r.preset == preset && r.set == set)
    }
}

/// One `benches/results.jsonl` line: a self-contained row (run metadata
/// denormalized in) so the tracking log can be sliced without joining.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackingRow {
    pub timestamp_epoch_s: u64,
    pub os: String,
    pub arch: String,
    pub corpus_seed: u64,
    pub corpus_scale: f64,
    #[serde(flatten)]
    pub result: BenchResult,
}

impl TrackingRow {
    pub fn from_run(run: &BenchRun) -> Vec<Self> {
        run.results
            .iter()
            .map(|r| TrackingRow {
                timestamp_epoch_s: run.timestamp_epoch_s,
                os: run.machine.os.clone(),
                arch: run.machine.arch.clone(),
                corpus_seed: run.corpus.seed,
                corpus_scale: run.corpus.scale,
                result: r.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(tool: &str) -> BenchResult {
        BenchResult {
            tool: tool.to_string(),
            format: "tar.zst".to_string(),
            preset: "fast".to_string(),
            level: 3,
            set: "text".to_string(),
            in_bytes: 1_048_576,
            out_bytes: 262_144,
            compress_ms: 500,
            decompress_ms: 250,
            reps: 3,
        }
    }

    #[test]
    fn derived_metrics() {
        let r = result("squash");
        assert_eq!(r.ratio(), 0.25);
        assert_eq!(r.compress_mib_s(), 2.0);
        assert_eq!(r.decompress_mib_s(), 4.0);
    }

    #[test]
    fn zero_time_is_infinite_not_nan() {
        let mut r = result("squash");
        r.compress_ms = 0;
        assert!(r.compress_mib_s().is_infinite());
    }

    #[test]
    fn lookup_matches_full_key() {
        let run = BenchRun {
            version: RUN_SCHEMA_VERSION,
            timestamp_epoch_s: 0,
            machine: MachineInfo {
                os: "macos".into(),
                arch: "aarch64".into(),
                cores: 8,
                cpu: None,
                ram_bytes: None,
            },
            corpus: CorpusInfo {
                seed: 42,
                scale: 1.0,
                total_bytes: 0,
                sets: vec!["text".into()],
            },
            skipped: vec![],
            results: vec![result("squash"), result("gzip")],
        };
        assert!(run.results_for("gzip", "tar.zst", "fast", "text").is_some());
        assert!(run
            .results_for("squash", "tar.zst", "max", "text")
            .is_none());
    }

    #[test]
    fn tracking_rows_denormalize_run_metadata() {
        let run = BenchRun {
            version: RUN_SCHEMA_VERSION,
            timestamp_epoch_s: 123,
            machine: MachineInfo {
                os: "linux".into(),
                arch: "x86_64".into(),
                cores: 4,
                cpu: None,
                ram_bytes: None,
            },
            corpus: CorpusInfo {
                seed: 42,
                scale: 0.5,
                total_bytes: 0,
                sets: vec![],
            },
            skipped: vec![],
            results: vec![result("squash")],
        };
        let rows = TrackingRow::from_run(&run);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].timestamp_epoch_s, 123);
        assert_eq!(rows[0].corpus_scale, 0.5);
        assert_eq!(rows[0].result.tool, "squash");
        // Must serialize as one flat JSON object per line.
        let line = serde_json::to_string(&rows[0]).unwrap();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["tool"], "squash");
        assert_eq!(value["corpus_seed"], 42);
    }
}
