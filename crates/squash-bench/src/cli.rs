//! Command-line surface and run orchestration.
//!
//! `run` and `compare` share one pipeline ([`run_benchmark`]) so the squash
//! numbers are produced identically in both modes; `compare` only adds the
//! competitor cells. Markdown goes to stdout (humans, release pages), JSON
//! to `--json` (tracking), one flat row per result to the JSONL log.

use crate::competitors::CompetitorRunner;
use crate::corpus::{self, Manifest};
use crate::levels;
use crate::machine;
use crate::model::{BenchResult, BenchRun, CorpusInfo, TrackingRow, RUN_SCHEMA_VERSION};
use crate::report;
use crate::runner::{preset_name, SquashRunner};
use clap::{Parser, Subcommand};
use squash_core::format::Format;
use squash_core::presets::Preset;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Formats the harness benchmarks: the docs/01 §3.3 MVP compress list.
/// Single-file codecs (gz/xz/zst) take exactly one file per job, so they
/// don't apply to directory corpus sets.
pub const BENCH_FORMATS: [Format; 4] = [Format::Zip, Format::SevenZ, Format::TarGz, Format::TarZst];

#[derive(Parser)]
#[command(
    name = "squash-bench",
    about = "Squash benchmark harness vs 7-Zip/gzip/zstd/xz (docs/05 §6)",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage the standard corpus (deterministic, seeded).
    Corpus {
        #[command(subcommand)]
        action: CorpusAction,
    },
    /// Benchmark Squash only, via the core Engine.
    Run(BenchArgs),
    /// Benchmark Squash plus every competitor found on PATH (the rest are
    /// reported as skipped, never silently dropped).
    Compare(BenchArgs),
    /// Render the markdown report from a run JSON file.
    Report {
        /// Run JSON produced by `run`/`compare --json`.
        #[arg(long)]
        input: PathBuf,
    },
    /// CI gate hook: fail (exit 1) if squash results regress vs baseline
    /// beyond the tolerance (ratio or speed). Not wired into CI yet.
    Check {
        /// Run JSON to evaluate.
        #[arg(long)]
        input: PathBuf,
        /// Baseline run JSON (the committed v0 reference).
        #[arg(long, default_value = "benches/baseline.json")]
        baseline: PathBuf,
        /// Allowed regression in percent (docs/05 §6 gate: 2%).
        #[arg(long, default_value_t = crate::check::DEFAULT_TOLERANCE_PCT)]
        tolerance_pct: f64,
    },
}

#[derive(Subcommand)]
pub enum CorpusAction {
    /// Generate the corpus deterministically (byte-identical everywhere for
    /// a given seed+scale). Scale 1.0 ≈ 245 MiB; use small scales for smoke.
    Generate {
        #[arg(long, default_value = "benches/corpus")]
        dir: PathBuf,
        #[arg(long, default_value_t = corpus::DEFAULT_SEED)]
        seed: u64,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
    },
}

#[derive(clap::Args)]
pub struct BenchArgs {
    /// Corpus directory produced by `corpus generate`.
    #[arg(long, default_value = "benches/corpus")]
    pub corpus: PathBuf,
    /// Corpus sets to benchmark (comma-separated; default: all).
    #[arg(long, value_delimiter = ',')]
    pub sets: Vec<String>,
    /// Formats (comma-separated subset of zip,7z,tar.gz,tar.zst; default: all).
    #[arg(long, value_delimiter = ',')]
    pub formats: Vec<String>,
    /// Presets (comma-separated subset of fast,balanced,max; default: all).
    #[arg(long, value_delimiter = ',')]
    pub presets: Vec<String>,
    /// Timed repetitions per cell (median is reported).
    #[arg(long, default_value_t = 3)]
    pub reps: u32,
    /// Untimed warmup runs before the timed reps.
    #[arg(long, default_value_t = 1)]
    pub warmup: u32,
    /// Write the full run JSON here.
    #[arg(long)]
    pub json: Option<PathBuf>,
    /// Append one tracking row per result to this JSONL log.
    #[arg(long, default_value = "benches/results.jsonl")]
    pub log: PathBuf,
    /// Do not append to the tracking log.
    #[arg(long)]
    pub no_log: bool,
    /// Scratch space for archives/tars/extraction (gitignored).
    #[arg(long, default_value = "target/bench-work")]
    pub work_dir: PathBuf,
}

/// Everything the smoke test and the CLI both need to run a benchmark.
pub struct BenchOptions {
    pub corpus_dir: PathBuf,
    pub sets: Vec<String>,
    pub formats: Vec<Format>,
    pub presets: Vec<Preset>,
    pub reps: u32,
    pub warmup: u32,
    pub work_dir: PathBuf,
    pub with_competitors: bool,
}

pub fn parse_formats(names: &[String]) -> Result<Vec<Format>, String> {
    if names.is_empty() {
        return Ok(BENCH_FORMATS.to_vec());
    }
    names
        .iter()
        .map(|n| {
            let format = Format::from_str(n).map_err(|_| format!("unknown format: {n}"))?;
            if BENCH_FORMATS.contains(&format) {
                Ok(format)
            } else {
                Err(format!(
                    "{n} is not benchmarkable (single-file codecs take one file; \
                     extract-only formats have no create path); choose from: {}",
                    BENCH_FORMATS
                        .iter()
                        .map(|f| f.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        })
        .collect()
}

pub fn parse_presets(names: &[String]) -> Result<Vec<Preset>, String> {
    if names.is_empty() {
        return Ok(Preset::ALL.to_vec());
    }
    names
        .iter()
        .map(|n| {
            Preset::ALL
                .into_iter()
                .find(|p| preset_name(*p) == n)
                .ok_or_else(|| format!("unknown preset: {n} (fast|balanced|max)"))
        })
        .collect()
}

fn epoch_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The shared run/compare pipeline: squash cells always, competitor cells
/// when `with_competitors` (missing tools and failed cells become skip
/// lines, never aborts).
pub fn run_benchmark(opts: &BenchOptions) -> Result<BenchRun, String> {
    let manifest = corpus::load_manifest(&opts.corpus_dir)
        .map_err(|e| format!("reading corpus manifest: {e}"))?
        .ok_or_else(|| {
            format!(
                "no manifest.json in {} — run `squash-bench corpus generate` first",
                opts.corpus_dir.display()
            )
        })?;
    if opts.reps == 0 {
        return Err("--reps must be >= 1".to_string());
    }
    let sets = if opts.sets.is_empty() {
        manifest.sets.iter().map(|s| s.name.clone()).collect()
    } else {
        for s in &opts.sets {
            if !manifest.sets.iter().any(|m| &m.name == s) {
                return Err(format!("corpus has no set `{s}`"));
            }
        }
        opts.sets.clone()
    };

    let mut results: Vec<BenchResult> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    let squash = SquashRunner::new(&opts.work_dir.join("squash")).map_err(|e| e.to_string())?;
    for set in &sets {
        let set_dir = opts.corpus_dir.join(set);
        for format in &opts.formats {
            for preset in &opts.presets {
                eprintln!("[squash] {set} {} {}", format.name(), preset_name(*preset));
                let row = squash
                    .bench(&set_dir, set, *format, *preset, opts.warmup, opts.reps)
                    .map_err(|e| {
                        format!(
                            "squash {set}/{} /{}: {e}",
                            format.name(),
                            preset_name(*preset)
                        )
                    })?;
                results.push(row);
            }
        }
    }

    if opts.with_competitors {
        let (detected, detect_skips) = levels::detect_all();
        skipped.extend(detect_skips);
        let comp =
            CompetitorRunner::new(&opts.work_dir.join("competitors")).map_err(|e| e.to_string())?;
        let tar_supported = detected.iter().any(|d| d.tool.needs_tar_input());
        for set in &sets {
            let content_bytes = manifest_bytes(&manifest, set);
            // One shared tar per set for the codec CLIs; not timed.
            let tar = if tar_supported {
                match comp.tar_of(&opts.corpus_dir, set) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        skipped.push(format!("tar prep for `{set}`: failed ({e}) — codec competitors skipped for this set"));
                        None
                    }
                }
            } else {
                None
            };
            for det in &detected {
                if det.tool.needs_tar_input() && tar.is_none() {
                    continue;
                }
                for (format, label) in det.tool.compared_formats() {
                    for preset in &opts.presets {
                        let level = levels::competitor_level(*format, *preset)
                            .expect("compared formats are create-capable");
                        let input = if det.tool.needs_tar_input() {
                            tar.clone().expect("checked above")
                        } else {
                            opts.corpus_dir.join(set)
                        };
                        eprintln!(
                            "[{}] {set} {label} {}",
                            det.tool.label(),
                            preset_name(*preset)
                        );
                        let spec = crate::competitors::CellSpec {
                            format_label: label,
                            level,
                            preset: preset_name(*preset),
                            set,
                            input,
                            content_bytes,
                            warmup: opts.warmup,
                            reps: opts.reps,
                        };
                        match comp.bench(det, &spec) {
                            Ok(row) => results.push(row),
                            Err(e) => skipped.push(format!(
                                "{} {label} {} on `{set}`: failed ({e})",
                                det.tool.label(),
                                preset_name(*preset)
                            )),
                        }
                    }
                }
            }
        }
    }

    Ok(BenchRun {
        version: RUN_SCHEMA_VERSION,
        timestamp_epoch_s: epoch_now(),
        machine: machine::collect(),
        corpus: CorpusInfo {
            seed: manifest.seed,
            scale: manifest.scale,
            total_bytes: manifest
                .sets
                .iter()
                .filter(|s| sets.contains(&s.name))
                .map(|s| s.bytes)
                .sum(),
            sets,
        },
        skipped,
        results,
    })
}

fn manifest_bytes(manifest: &Manifest, set: &str) -> u64 {
    manifest
        .sets
        .iter()
        .find(|s| s.name == set)
        .map(|s| s.bytes)
        .unwrap_or(0)
}

fn append_tracking_log(path: &Path, run: &BenchRun) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;
    for row in TrackingRow::from_run(run) {
        let line = serde_json::to_string(&row).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn read_run(path: &Path) -> Result<BenchRun, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
}

/// CLI entry point; returns the process exit code.
pub fn run_cli(cli: Cli) -> i32 {
    let with_competitors = matches!(cli.command, Commands::Compare(_));
    match cli.command {
        Commands::Corpus {
            action: CorpusAction::Generate { dir, seed, scale },
        } => match corpus::generate(&dir, seed, scale) {
            Ok(manifest) => {
                println!(
                    "corpus generated in {}: {} sets, {:.1} MiB (seed {seed}, scale {scale})",
                    dir.display(),
                    manifest.sets.len(),
                    manifest.total_bytes as f64 / 1048576.0,
                );
                for set in &manifest.sets {
                    println!(
                        "  {:<12} {:>5} files {:>10.1} MiB  fnv1a64 {}",
                        set.name,
                        set.files,
                        set.bytes as f64 / 1048576.0,
                        set.fnv1a64,
                    );
                }
                0
            }
            Err(e) => {
                eprintln!("corpus generate failed: {e}");
                1
            }
        },
        Commands::Run(args) | Commands::Compare(args) => {
            let opts = BenchOptions {
                corpus_dir: args.corpus.clone(),
                sets: args.sets.clone(),
                formats: match parse_formats(&args.formats) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("{e}");
                        return 2;
                    }
                },
                presets: match parse_presets(&args.presets) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("{e}");
                        return 2;
                    }
                },
                reps: args.reps,
                warmup: args.warmup,
                work_dir: args.work_dir.clone(),
                with_competitors,
            };
            match run_benchmark(&opts) {
                Ok(run) => {
                    print!("{}", report::markdown(&run));
                    if let Some(json) = &args.json {
                        match serde_json::to_string_pretty(&run) {
                            Ok(text) => {
                                if let Err(e) = fs::write(json, text) {
                                    eprintln!("writing {}: {e}", json.display());
                                    return 1;
                                }
                            }
                            Err(e) => {
                                eprintln!("serializing run: {e}");
                                return 1;
                            }
                        }
                    }
                    if !args.no_log {
                        if let Err(e) = append_tracking_log(&args.log, &run) {
                            eprintln!("appending tracking log: {e}");
                            return 1;
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("benchmark failed: {e}");
                    1
                }
            }
        }
        Commands::Report { input } => match read_run(&input) {
            Ok(run) => {
                print!("{}", report::markdown(&run));
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        Commands::Check {
            input,
            baseline,
            tolerance_pct,
        } => {
            let baseline_run = match read_run(&baseline) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("check: baseline: {e}");
                    return 1;
                }
            };
            let current_run = match read_run(&input) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("check: input: {e}");
                    return 1;
                }
            };
            let report = crate::check::check(&baseline_run, &current_run, tolerance_pct);
            if !report.corpus_match {
                eprintln!(
                    "check FAILED: corpus mismatch (baseline seed {} scale {}, run seed {} scale {}) — regenerate one to match the other",
                    baseline_run.corpus.seed,
                    baseline_run.corpus.scale,
                    current_run.corpus.seed,
                    current_run.corpus.scale,
                );
                return 1;
            }
            for r in &report.regressions {
                eprintln!(
                    "regression: {} {} {:.4} → {:.4} ({:+.1}% worse than the {:.1}% gate)",
                    r.key, r.metric, r.baseline, r.current, r.worse_by_pct, tolerance_pct
                );
            }
            if report.passed() {
                println!(
                    "check PASSED: {} squash cells within {:.1}% of baseline",
                    report.compared, tolerance_pct
                );
                0
            } else {
                eprintln!(
                    "check FAILED: {} regressions in {} compared cells",
                    report.regressions.len(),
                    report.compared
                );
                1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_formats_defaults_and_validates() {
        assert_eq!(parse_formats(&[]).unwrap(), BENCH_FORMATS.to_vec());
        assert_eq!(
            parse_formats(&["zip".to_string(), "tar.zst".to_string()]).unwrap(),
            vec![Format::Zip, Format::TarZst]
        );
        assert!(parse_formats(&["rar".to_string()]).is_err());
        assert!(parse_formats(&["gz".to_string()]).is_err());
        assert!(parse_formats(&["nope".to_string()]).is_err());
    }

    #[test]
    fn parse_presets_defaults_and_validates() {
        assert_eq!(parse_presets(&[]).unwrap(), Preset::ALL.to_vec());
        assert_eq!(
            parse_presets(&["fast".to_string(), "max".to_string()]).unwrap(),
            vec![Preset::Fast, Preset::Max]
        );
        assert!(parse_presets(&["ultra".to_string()]).is_err());
    }
}
