//! Human-facing markdown report and the docs/01 §4 claim checks.
//!
//! The table is flat and sortable-by-eye: one section per corpus set, rows
//! grouped tool → format → preset. Claim checks only evaluate pairs where
//! both sides were actually measured — when `7zz` is absent the report says
//! so instead of implying a verdict.

use crate::model::BenchRun;
use std::fmt::Write as _;

pub fn markdown(run: &BenchRun) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Squash benchmark — {}",
        display_date(run.timestamp_epoch_s)
    );
    let m = &run.machine;
    let _ = writeln!(
        out,
        "\nMachine: {} {}, {} cores{}{}",
        m.os,
        m.arch,
        m.cores,
        m.cpu
            .as_deref()
            .map(|c| format!(", {c}"))
            .unwrap_or_default(),
        m.ram_bytes
            .map(|b| format!(", {:.1} GiB", b as f64 / 1073741824.0))
            .unwrap_or_default(),
    );
    let _ = writeln!(
        out,
        "Corpus: seed {}, scale {}, {} sets, {:.1} MiB | reps: {}",
        run.corpus.seed,
        run.corpus.scale,
        run.corpus.sets.len(),
        run.corpus.total_bytes as f64 / 1048576.0,
        run.results.first().map(|r| r.reps).unwrap_or(0),
    );
    if !run.skipped.is_empty() {
        let _ = writeln!(out, "\n**Skipped:**");
        for line in &run.skipped {
            let _ = writeln!(out, "- {line}");
        }
    }

    let _ = writeln!(out, "\n## Results");
    for set in &run.corpus.sets {
        let mut rows: Vec<_> = run.results.iter().filter(|r| &r.set == set).collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort_by(|a, b| (&a.tool, &a.format, &a.preset).cmp(&(&b.tool, &b.format, &b.preset)));
        let _ = writeln!(
            out,
            "\n### set: {set} ({:.1} MiB)",
            rows[0].in_bytes as f64 / 1048576.0
        );
        let _ = writeln!(
            out,
            "| tool | format | preset | level | out (MiB) | ratio | compress (MiB/s) | decompress (MiB/s) |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
        for r in rows {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {:.2} | {:.1}% | {:.1} | {:.1} |",
                r.tool,
                r.format,
                r.preset,
                r.level,
                r.out_bytes as f64 / 1048576.0,
                r.ratio() * 100.0,
                r.compress_mib_s(),
                r.decompress_mib_s(),
            );
        }
    }

    let _ = writeln!(out, "\n## Claim checks (docs/01 §4)");
    for line in claim_checks(run) {
        let _ = writeln!(out, "- {line}");
    }
    out
}

/// Evaluate the two provable product claims where data exists:
/// 1. balanced (competitor-default) ratio within 5% of 7-Zip's default;
/// 2. zstd's compress speed win over the gzip baseline.
pub fn claim_checks(run: &BenchRun) -> Vec<String> {
    let mut lines = Vec::new();
    let have_7zz = run.results.iter().any(|r| r.tool == "7zz");
    if have_7zz {
        for set in &run.corpus.sets {
            let sq = run.results_for("squash", "7z", "balanced", set);
            let sz = run.results_for("7zz", "7z", "balanced", set);
            if let (Some(sq), Some(sz)) = (sq, sz) {
                let ratio_delta = (sq.ratio() - sz.ratio()) / sz.ratio() * 100.0;
                let verdict = if ratio_delta <= 5.0 { "PASS" } else { "FAIL" };
                lines.push(format!(
                    "7z balanced vs 7-Zip default (-mx=5) on `{set}`: ratio {:.1}% vs {:.1}% ({:+.1}%), squash {:.1} vs {:.1} MiB/s — {verdict} (docs/01 §5: within 5%)",
                    sq.ratio() * 100.0,
                    sz.ratio() * 100.0,
                    ratio_delta,
                    sq.compress_mib_s(),
                    sz.compress_mib_s(),
                ));
            }
        }
    } else {
        lines.push(
            "7-Zip ratio claim: NOT EVALUATED — 7zz not on PATH (install p7zip to measure)"
                .to_string(),
        );
    }

    let have_gzip = run.results.iter().any(|r| r.tool == "gzip");
    if have_gzip {
        for set in &run.corpus.sets {
            let zst = run.results_for("squash", "tar.zst", "fast", set);
            let gz = run.results_for("gzip", "tar.gz", "balanced", set);
            if let (Some(zst), Some(gz)) = (zst, gz) {
                lines.push(format!(
                    "zstd speed on `{set}`: squash tar.zst fast (zstd-3) compresses at {:.1} MiB/s vs gzip -6 at {:.1} MiB/s ({:.1}×), ratio {:.1}% vs {:.1}%",
                    zst.compress_mib_s(),
                    gz.compress_mib_s(),
                    zst.compress_mib_s() / gz.compress_mib_s(),
                    zst.ratio() * 100.0,
                    gz.ratio() * 100.0,
                ));
            }
        }
    } else {
        lines.push("zstd speed claim: NOT EVALUATED — gzip baseline absent".to_string());
    }
    lines
}

/// UTC timestamp for the report header, via the system `date` (keeps chrono
/// out of the dependency tree); falls back to raw epoch seconds. Tries the
/// BSD (`-r`) then the GNU (`-d @`) form.
fn display_date(epoch_s: u64) -> String {
    let forms = [
        vec!["-u".to_string(), "-r".to_string(), epoch_s.to_string()],
        vec!["-u".to_string(), "-d".to_string(), format!("@{epoch_s}")],
    ];
    for args in forms {
        let rendered = std::process::Command::new("date")
            .args(&args)
            .arg("+%Y-%m-%dT%H:%M:%SZ")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(s) = rendered {
            return s;
        }
    }
    format!("unix {epoch_s}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BenchResult, CorpusInfo, MachineInfo, RUN_SCHEMA_VERSION};

    fn row(
        tool: &str,
        format: &str,
        preset: &str,
        in_b: u64,
        out_b: u64,
        cms: u64,
        dms: u64,
    ) -> BenchResult {
        BenchResult {
            tool: tool.into(),
            format: format.into(),
            preset: preset.into(),
            level: 5,
            set: "text".into(),
            in_bytes: in_b,
            out_bytes: out_b,
            compress_ms: cms,
            decompress_ms: dms,
            reps: 3,
        }
    }

    fn run_with(results: Vec<BenchResult>, skipped: Vec<String>) -> BenchRun {
        BenchRun {
            version: RUN_SCHEMA_VERSION,
            timestamp_epoch_s: 1_786_000_000,
            machine: MachineInfo {
                os: "macos".into(),
                arch: "aarch64".into(),
                cores: 10,
                cpu: Some("Apple M-test".into()),
                ram_bytes: Some(16 * 1073741824),
            },
            corpus: CorpusInfo {
                seed: 42,
                scale: 0.1,
                total_bytes: 2_097_152,
                sets: vec!["text".into()],
            },
            skipped,
            results,
        }
    }

    #[test]
    fn report_renders_table_and_skips() {
        let run = run_with(
            vec![row(
                "squash", "tar.zst", "fast", 1_048_576, 262_144, 100, 50,
            )],
            vec!["7zz: skipped (not on PATH)".into()],
        );
        let md = markdown(&run);
        assert!(md.contains("# Squash benchmark"));
        assert!(md.contains("7zz: skipped (not on PATH)"));
        assert!(md.contains("| squash | tar.zst | fast |"));
        assert!(md.contains("25.0%"));
        assert!(md.contains("### set: text"));
        // No 7zz rows → the ratio claim must say so, not imply a verdict.
        assert!(md.contains("NOT EVALUATED"));
    }

    #[test]
    fn claim_check_passes_within_five_percent() {
        // squash ratio 0.500 vs 7zz 0.480 → +4.2% → PASS (within 5%).
        let run = run_with(
            vec![
                row("squash", "7z", "balanced", 1_000_000, 500_000, 100, 100),
                row("7zz", "7z", "balanced", 1_000_000, 480_000, 100, 100),
            ],
            vec![],
        );
        let lines = claim_checks(&run);
        assert!(lines.iter().any(|l| l.contains("PASS")), "{lines:?}");
    }

    #[test]
    fn claim_check_fails_beyond_five_percent() {
        let run = run_with(
            vec![
                row("squash", "7z", "balanced", 1_000_000, 560_000, 100, 100),
                row("7zz", "7z", "balanced", 1_000_000, 480_000, 100, 100),
            ],
            vec![],
        );
        let lines = claim_checks(&run);
        assert!(lines.iter().any(|l| l.contains("FAIL")), "{lines:?}");
    }

    #[test]
    fn zstd_speed_claim_compares_against_gzip_default() {
        let run = run_with(
            vec![
                row("squash", "tar.zst", "fast", 1_048_576, 400_000, 10, 5),
                row("gzip", "tar.gz", "balanced", 1_048_576, 300_000, 100, 50),
            ],
            vec![],
        );
        let lines = claim_checks(&run);
        let line = lines.iter().find(|l| l.contains("zstd speed")).unwrap();
        assert!(line.contains("10.0×"), "{line}");
    }
}
