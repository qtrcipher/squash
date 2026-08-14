//! Competitor-side runner: times real CLI processes (`7zz`, `gzip`, `zstd`,
//! `xz`) with the same warmup/reps/median discipline as the squash runner.
//! Codec CLIs consume a tar of the corpus set (single-stream tools can't
//! archive directories); 7zz archives the set directory directly. Output is
//! always written to disk — no `/dev/null` shortcuts — so both sides pay
//! the same I/O cost.

use crate::levels::{compress_args, decompress_args, Detected};
use crate::model::BenchResult;
use crate::prng::median_ms;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

pub struct CompetitorRunner {
    work_dir: PathBuf,
}

/// One competitor benchmark cell: what to run, on what input, how often.
pub struct CellSpec<'a> {
    pub format_label: &'a str,
    pub level: u8,
    pub preset: &'a str,
    pub set: &'a str,
    /// Set directory (7zz) or the prepared tar of the set (codec CLIs).
    pub input: PathBuf,
    /// The set's uncompressed content size — the ratio/speed denominator
    /// for every tool, so rows compare like-for-like.
    pub content_bytes: u64,
    pub warmup: u32,
    pub reps: u32,
}

impl CompetitorRunner {
    pub fn new(work_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(work_dir)?;
        Ok(Self {
            work_dir: work_dir.to_path_buf(),
        })
    }

    /// Create (or reuse) the tar of a corpus set that codec CLIs compress.
    /// Built once per set per run and shared by every codec competitor, so
    /// their inputs are byte-identical. Not timed.
    pub fn tar_of(&self, corpus_dir: &Path, set: &str) -> Result<PathBuf, String> {
        let tar_path = self.work_dir.join("tars").join(format!("{set}.tar"));
        if tar_path.exists() {
            return Ok(tar_path);
        }
        fs::create_dir_all(tar_path.parent().expect("tars dir")).map_err(|e| e.to_string())?;
        // COPYFILE_DISABLE keeps macOS bsdtar from adding AppleDouble
        // entries; the tar is a shared input, not a timed artifact.
        let status = Command::new("tar")
            .arg("-cf")
            .arg(&tar_path)
            .arg("-C")
            .arg(corpus_dir)
            .arg(set)
            .env("COPYFILE_DISABLE", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("failed to spawn tar: {e}"))?;
        if !status.success() {
            return Err(format!("tar -cf {set}.tar exited with {status}"));
        }
        Ok(tar_path)
    }

    /// Benchmark one competitor cell.
    pub fn bench(&self, det: &Detected, spec: &CellSpec) -> Result<BenchResult, String> {
        let cell = format!(
            "{}-{}-{}-{}",
            det.tool.label(),
            spec.set,
            spec.format_label,
            spec.preset
        );
        let archive = self.work_dir.join(format!("{cell}.out"));
        let extract_dir = self.work_dir.join(format!("{cell}.extract"));
        let extract_file = self.work_dir.join(format!("{cell}.extracted"));

        let compress =
            |out: &Path| self.compress_once(det, spec.level, &spec.input, out, spec.format_label);
        let decompress = |archive: &Path| {
            if det.tool.needs_tar_input() {
                self.decompress_to_file(det, archive, &extract_file)
            } else {
                self.decompress_to_dir(det, archive, &extract_dir)
            }
        };

        for _ in 0..spec.warmup {
            compress(&archive)?;
        }
        let mut compress_samples = Vec::with_capacity(spec.reps as usize);
        for _ in 0..spec.reps {
            let start = Instant::now();
            compress(&archive)?;
            compress_samples.push(start.elapsed().as_millis());
        }
        let out_bytes = fs::metadata(&archive).map_err(|e| e.to_string())?.len();

        for _ in 0..spec.warmup {
            decompress(&archive)?;
        }
        let mut decompress_samples = Vec::with_capacity(spec.reps as usize);
        for _ in 0..spec.reps {
            let start = Instant::now();
            decompress(&archive)?;
            decompress_samples.push(start.elapsed().as_millis());
        }

        let _ = fs::remove_file(&archive);
        let _ = fs::remove_dir_all(&extract_dir);
        let _ = fs::remove_file(&extract_file);

        Ok(BenchResult {
            tool: det.tool.label().to_string(),
            format: spec.format_label.to_string(),
            preset: spec.preset.to_string(),
            level: spec.level,
            set: spec.set.to_string(),
            in_bytes: spec.content_bytes,
            out_bytes,
            compress_ms: median_ms(&mut compress_samples),
            decompress_ms: median_ms(&mut decompress_samples),
            reps: spec.reps,
        })
    }

    fn compress_once(
        &self,
        det: &Detected,
        level: u8,
        input: &Path,
        out: &Path,
        format_label: &str,
    ) -> Result<(), String> {
        let _ = fs::remove_file(out);
        let format = match format_label {
            "zip" => squash_core::Format::Zip,
            "7z" => squash_core::Format::SevenZ,
            "tar.gz" => squash_core::Format::TarGz,
            "tar.zst" => squash_core::Format::TarZst,
            _ => squash_core::Format::Xz,
        };
        let mut cmd = Command::new(&det.binary);
        cmd.args(compress_args(det.tool, format, level));
        if det.tool.writes_to_stdout() {
            let file = File::create(out).map_err(|e| e.to_string())?;
            cmd.arg(input).stdout(Stdio::from(file));
        } else {
            cmd.arg(out).arg(input).stdout(Stdio::null());
        }
        run(cmd).map_err(|e| format!("{} compress: {e}", det.tool.label()))
    }

    fn decompress_to_file(&self, det: &Detected, archive: &Path, out: &Path) -> Result<(), String> {
        let _ = fs::remove_file(out);
        let file = File::create(out).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&det.binary);
        cmd.args(decompress_args(det.tool))
            .arg(archive)
            .stdout(Stdio::from(file));
        run(cmd).map_err(|e| format!("{} decompress: {e}", det.tool.label()))
    }

    fn decompress_to_dir(&self, det: &Detected, archive: &Path, dir: &Path) -> Result<(), String> {
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&det.binary);
        cmd.args(decompress_args(det.tool))
            .arg(format!("-o{}", dir.display()))
            .arg(archive)
            .stdout(Stdio::null());
        run(cmd).map_err(|e| format!("{} decompress: {e}", det.tool.label()))
    }
}

fn run(mut cmd: Command) -> Result<(), String> {
    let status = cmd
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("spawn failed: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("exited with {status}"))
    }
}
