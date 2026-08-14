//! Squash-side runner: times real `Engine` jobs (compress + extract) with
//! warmup and repetitions, reporting medians. Wall-clock is measured around
//! `submit` + `wait`, i.e. what a user of the core actually experiences —
//! not the handler's internal `JobStats::duration`.

use crate::model::BenchResult;
use crate::prng::median_ms;
use squash_core::format::Format;
use squash_core::job::Job;
use squash_core::presets::Preset;
use squash_core::{Engine, SquashError};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct SquashRunner {
    engine: Engine,
    work_dir: PathBuf,
}

impl SquashRunner {
    pub fn new(work_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(work_dir)?;
        Ok(Self {
            engine: Engine::new(),
            work_dir: work_dir.to_path_buf(),
        })
    }

    /// Benchmark one (set, format, preset) cell: `warmup` untimed runs, then
    /// `reps` timed runs of compress followed by the same for extract.
    pub fn bench(
        &self,
        input: &Path,
        set: &str,
        format: Format,
        preset: Preset,
        warmup: u32,
        reps: u32,
    ) -> Result<BenchResult, SquashError> {
        let cell = format!("{}-{}-{}", set, format.name(), preset_name(preset));
        let archive = self
            .work_dir
            .join(format!("{cell}.{}", format.extensions()[0]));
        let extract_dir = self.work_dir.join(format!("{cell}.extract"));

        // Compress: warmup, then timed reps. The last archive is kept for
        // the extract phase.
        let mut in_bytes = 0;
        for _ in 0..warmup {
            let stats = self.compress_once(input, &archive, format, preset)?;
            in_bytes = stats;
        }
        let mut compress_samples = Vec::with_capacity(reps as usize);
        for _ in 0..reps {
            let start = Instant::now();
            in_bytes = self.compress_once(input, &archive, format, preset)?;
            compress_samples.push(start.elapsed().as_millis());
        }
        let out_bytes = fs::metadata(&archive).map_err(map_io)?.len();

        // Extract: warmup, then timed reps into a fresh directory each time.
        for _ in 0..warmup {
            self.extract_once(&archive, &extract_dir, format)?;
        }
        let mut decompress_samples = Vec::with_capacity(reps as usize);
        for _ in 0..reps {
            let start = Instant::now();
            self.extract_once(&archive, &extract_dir, format)?;
            decompress_samples.push(start.elapsed().as_millis());
        }

        let _ = fs::remove_file(&archive);
        let _ = fs::remove_dir_all(&extract_dir);

        Ok(BenchResult {
            tool: "squash".to_string(),
            format: format.name().to_string(),
            preset: preset_name(preset).to_string(),
            level: squash_core::presets::params(format, preset)
                .map(|p| p.level)
                .unwrap_or(0),
            set: set.to_string(),
            in_bytes,
            out_bytes,
            compress_ms: median_ms(&mut compress_samples),
            decompress_ms: median_ms(&mut decompress_samples),
            reps,
        })
    }

    fn compress_once(
        &self,
        input: &Path,
        archive: &Path,
        format: Format,
        preset: Preset,
    ) -> Result<u64, SquashError> {
        let _ = fs::remove_file(archive);
        let handle = self.engine.submit(Job::compress(
            vec![input.to_path_buf()],
            archive.to_path_buf(),
            format,
            preset,
        ));
        let stats = handle.wait()?;
        Ok(stats.in_bytes)
    }

    fn extract_once(&self, archive: &Path, dest: &Path, format: Format) -> Result<(), SquashError> {
        let _ = fs::remove_dir_all(dest);
        fs::create_dir_all(dest).map_err(map_io)?;
        let handle = self.engine.submit(Job::extract(
            vec![archive.to_path_buf()],
            dest.to_path_buf(),
            format,
        ));
        handle.wait()?;
        Ok(())
    }
}

pub fn preset_name(preset: Preset) -> &'static str {
    match preset {
        Preset::Fast => "fast",
        Preset::Balanced => "balanced",
        Preset::Max => "max",
    }
}

fn map_io(err: io::Error) -> SquashError {
    match err.kind() {
        io::ErrorKind::PermissionDenied => SquashError::PermissionDenied,
        _ => SquashError::Internal,
    }
}
