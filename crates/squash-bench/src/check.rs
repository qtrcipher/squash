//! Regression gate vs `benches/baseline.json` (docs/05 §6: "fails CI on
//! regression > 2%"). Only `squash` rows are gated — competitor rows vary
//! with the host's installed tools and are context, not a contract.
//!
//! Not wired into CI yet; this is the hook: run `compare`, then
//! `squash-bench check --input <run.json> --baseline benches/baseline.json`
//! and gate on the exit code.

use crate::model::BenchRun;

#[derive(Debug, Clone, PartialEq)]
pub struct Regression {
    pub key: String,
    pub metric: &'static str,
    pub baseline: f64,
    pub current: f64,
    /// Signed percent change; positive = worse for every metric here
    /// (speeds are negated before comparison so one convention holds).
    pub worse_by_pct: f64,
}

#[derive(Debug, Default, PartialEq)]
pub struct CheckReport {
    /// False when baseline and run were generated from different corpora
    /// (seed/scale) — comparing them would be meaningless.
    pub corpus_match: bool,
    /// squash cells present in both files.
    pub compared: usize,
    pub regressions: Vec<Regression>,
}

impl CheckReport {
    pub fn passed(&self) -> bool {
        self.corpus_match && self.regressions.is_empty()
    }
}

pub const DEFAULT_TOLERANCE_PCT: f64 = 2.0;

/// Compare `current` against `baseline` with a tolerance in percent
/// (2.0 = docs/05 §6's gate). A cell regresses when its ratio grows or its
/// compress/decompress speed drops by more than the tolerance.
pub fn check(baseline: &BenchRun, current: &BenchRun, tolerance_pct: f64) -> CheckReport {
    let mut report = CheckReport {
        corpus_match: baseline.corpus.seed == current.corpus.seed
            && baseline.corpus.scale == current.corpus.scale,
        ..Default::default()
    };
    let tol = tolerance_pct / 100.0;
    for now in current.results.iter().filter(|r| r.tool == "squash") {
        let Some(base) = baseline.results_for(&now.tool, &now.format, &now.preset, &now.set) else {
            continue;
        };
        report.compared += 1;
        let key = format!("{}/{}/{}", now.format, now.preset, now.set);

        // Ratio: worse when it grows.
        if let Some(worse) = worse_by_growth(base.ratio(), now.ratio(), tol) {
            report.regressions.push(Regression {
                key: key.clone(),
                metric: "ratio",
                baseline: base.ratio(),
                current: now.ratio(),
                worse_by_pct: worse,
            });
        }
        // Speeds: worse when they shrink.
        for (metric, b, c) in [
            (
                "compress_mib_s",
                base.compress_mib_s(),
                now.compress_mib_s(),
            ),
            (
                "decompress_mib_s",
                base.decompress_mib_s(),
                now.decompress_mib_s(),
            ),
        ] {
            if let Some(worse) = worse_by_shrink(b, c, tol) {
                report.regressions.push(Regression {
                    key: key.clone(),
                    metric,
                    baseline: b,
                    current: c,
                    worse_by_pct: worse,
                });
            }
        }
    }
    report
}

/// `Some(% worse)` when current exceeds baseline * (1 + tol).
fn worse_by_growth(baseline: f64, current: f64, tol: f64) -> Option<f64> {
    if baseline <= 0.0 {
        return None;
    }
    let change = (current - baseline) / baseline;
    (change > tol).then_some(change * 100.0)
}

/// `Some(% worse)` when current drops below baseline * (1 - tol).
fn worse_by_shrink(baseline: f64, current: f64, tol: f64) -> Option<f64> {
    if baseline <= 0.0 {
        return None;
    }
    let change = (baseline - current) / baseline;
    (change > tol).then_some(change * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BenchResult, CorpusInfo, MachineInfo, RUN_SCHEMA_VERSION};

    fn row(tool: &str, out_b: u64, cms: u64, dms: u64) -> BenchResult {
        BenchResult {
            tool: tool.into(),
            format: "tar.zst".into(),
            preset: "fast".into(),
            level: 3,
            set: "text".into(),
            in_bytes: 1_000_000,
            out_bytes: out_b,
            compress_ms: cms,
            decompress_ms: dms,
            reps: 3,
        }
    }

    fn run(scale: f64, results: Vec<BenchResult>) -> BenchRun {
        BenchRun {
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
                scale,
                total_bytes: 1_000_000,
                sets: vec!["text".into()],
            },
            skipped: vec![],
            results,
        }
    }

    #[test]
    fn identical_runs_pass() {
        let base = run(1.0, vec![row("squash", 400_000, 100, 50)]);
        let now = run(1.0, vec![row("squash", 400_000, 100, 50)]);
        let report = check(&base, &now, DEFAULT_TOLERANCE_PCT);
        assert!(report.passed());
        assert_eq!(report.compared, 1);
    }

    #[test]
    fn ratio_regression_beyond_tolerance_fails() {
        let base = run(1.0, vec![row("squash", 400_000, 100, 50)]);
        // ratio 0.40 → 0.42 is +5%, beyond 2%.
        let now = run(1.0, vec![row("squash", 420_000, 100, 50)]);
        let report = check(&base, &now, DEFAULT_TOLERANCE_PCT);
        assert!(!report.passed());
        assert_eq!(report.regressions.len(), 1);
        assert_eq!(report.regressions[0].metric, "ratio");
    }

    #[test]
    fn speed_regression_beyond_tolerance_fails() {
        let base = run(1.0, vec![row("squash", 400_000, 100, 50)]);
        // compress 10 → 8 MiB/s is -20%.
        let now = run(1.0, vec![row("squash", 400_000, 125, 50)]);
        let report = check(&base, &now, DEFAULT_TOLERANCE_PCT);
        assert!(!report.passed());
        assert!(report
            .regressions
            .iter()
            .any(|r| r.metric == "compress_mib_s"));
    }

    #[test]
    fn within_tolerance_passes() {
        let base = run(1.0, vec![row("squash", 400_000, 100, 50)]);
        // +1% ratio, -1% compress speed, decompress unchanged: inside the gate.
        let now = run(1.0, vec![row("squash", 404_000, 101, 50)]);
        let report = check(&base, &now, DEFAULT_TOLERANCE_PCT);
        assert!(report.passed(), "{:?}", report.regressions);
    }

    #[test]
    fn competitor_rows_are_never_gated() {
        let base = run(1.0, vec![row("gzip", 400_000, 100, 50)]);
        let now = run(1.0, vec![row("gzip", 900_000, 10_000, 10_000)]);
        let report = check(&base, &now, DEFAULT_TOLERANCE_PCT);
        assert!(report.passed());
        assert_eq!(report.compared, 0);
    }

    #[test]
    fn corpus_mismatch_fails_closed() {
        let base = run(1.0, vec![row("squash", 400_000, 100, 50)]);
        let now = run(0.5, vec![row("squash", 400_000, 100, 50)]);
        let report = check(&base, &now, DEFAULT_TOLERANCE_PCT);
        assert!(!report.corpus_match);
        assert!(!report.passed());
    }

    #[test]
    fn cells_missing_from_baseline_are_skipped_not_failed() {
        let base = run(1.0, vec![]);
        let now = run(1.0, vec![row("squash", 400_000, 100, 50)]);
        let report = check(&base, &now, DEFAULT_TOLERANCE_PCT);
        assert!(report.passed());
        assert_eq!(report.compared, 0);
    }
}
