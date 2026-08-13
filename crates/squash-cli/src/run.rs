//! Dispatch: clap args → core `Job` → `Engine::submit` → rendered result.
//!
//! ## `--json` schema (JSONL, one object per line, stdout)
//!
//! The projection mirrors the job model one-to-one (docs/03 §4); the core's
//! `ProgressEvent` is deliberately not serde, so this schema is the stable
//! machine-readable contract:
//!
//! ```text
//! {"type":"job","id":1,"operation":"compress","inputs":["…"],"destination":"…","format":"zip","preset":"balanced"}
//! {"type":"started","total_bytes_estimate":123}              // null → indeterminate
//! {"type":"progress","bytes_done":N,"entries_done":N,"current_path":"…"}
//! {"type":"result","status":"finished","in_bytes":N,"out_bytes":N,"duration_ms":N}
//! {"type":"result","status":"failed","error":"corrupt_archive"}   // stable SquashError code
//! ```
//!
//! Human mode renders a progress bar on stderr (auto-hidden when not a TTY)
//! and one summary line per job on stdout.

use crate::cli::{Cli, Commands};
use crate::exit_codes;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::Serialize;
use squash_core::presets::Preset;
use squash_core::{
    Engine, Format, Job, JobHandle, JobStats, Operation, ProgressEvent, SquashError,
};
use std::path::{Path, PathBuf};

pub fn run(args: Cli) -> i32 {
    let engine = Engine::new();
    match &args.command {
        Commands::Compress {
            inputs,
            output,
            format,
            preset,
        } => run_compress(&engine, inputs, output, format, (*preset).into(), args.json),
        Commands::Extract { inputs, output } => run_extract(&engine, inputs, output, args.json),
    }
}

fn run_compress(
    engine: &Engine,
    inputs: &[PathBuf],
    output: &Option<PathBuf>,
    format_arg: &Option<String>,
    preset: Preset,
    json: bool,
) -> i32 {
    for input in inputs {
        if !input.exists() {
            return fail(
                json,
                &format!("input not found: {}", input.display()),
                exit_codes::USAGE,
            );
        }
    }
    let format = match resolve_compress_format(engine, output, format_arg) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let creatable = format.can_create()
        && engine
            .registry()
            .handler_for(format)
            .map(|h| h.can_create())
            .unwrap_or(false);
    if !creatable {
        let hint = if format == Format::Rar {
            // docs/05 §4: the RARLAB license forbids RAR creation, forever.
            " (the RAR license forbids creating rar — use 7z or tar.zst instead)"
        } else {
            ""
        };
        return fail(
            json,
            &format!("Squash cannot create '{format}' archives{hint}"),
            exit_codes::USAGE,
        );
    }
    if format.is_single_file() {
        return run_compress_single_file(engine, inputs, output, format, preset, json);
    }
    let destination = output
        .clone()
        .unwrap_or_else(|| default_output(inputs, format));

    let job = Job::compress(inputs.to_vec(), destination, format, preset);
    let handle = engine.submit(job.clone());
    drive(&handle, &job, json, None)
}

/// Single-file codecs (gz/xz/zst) take exactly one regular file per job.
/// Multiple inputs follow the docs/03 F4 default — one output per input
/// (`<name>.<ext>` next to each source) — so `-o` is only valid with one.
fn run_compress_single_file(
    engine: &Engine,
    inputs: &[PathBuf],
    output: &Option<PathBuf>,
    format: Format,
    preset: Preset,
    json: bool,
) -> i32 {
    for input in inputs {
        if input.is_dir() {
            return fail(
                json,
                &format!(
                    "{format} compresses a single file — use tar.gz or tar.zst for directories"
                ),
                exit_codes::USAGE,
            );
        }
    }
    if inputs.len() > 1 && output.is_some() {
        return fail(
            json,
            "-o takes one output path; single-file formats write one output per input",
            exit_codes::USAGE,
        );
    }
    let mut exit = exit_codes::SUCCESS;
    for input in inputs {
        let destination = output
            .clone()
            .unwrap_or_else(|| default_output(std::slice::from_ref(input), format));
        let job = Job::compress(vec![input.clone()], destination, format, preset);
        let handle = engine.submit(job.clone());
        exit = exit.max(drive(&handle, &job, json, None));
    }
    exit
}

fn run_extract(engine: &Engine, inputs: &[PathBuf], output: &Option<PathBuf>, json: bool) -> i32 {
    let mut exit = exit_codes::SUCCESS;
    for archive in inputs {
        if !archive.exists() {
            exit = exit.max(fail(
                json,
                &format!("input not found: {}", archive.display()),
                exit_codes::USAGE,
            ));
            continue;
        }
        let Some(format) = engine.registry().detect(archive) else {
            exit = exit.max(fail(
                json,
                &format!("cannot determine the format of {}", archive.display()),
                exit_codes::USAGE,
            ));
            continue;
        };
        let destination = output.clone().unwrap_or_else(|| {
            archive
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        let job = Job::extract(vec![archive.clone()], destination, format);
        let handle = engine.submit(job.clone());
        let code = drive(&handle, &job, json, Some(archive));
        exit = exit.max(code);
    }
    exit
}

/// `--format` wins; otherwise the output extension; otherwise zip (the
/// settings default, docs/06 §2).
fn resolve_compress_format(
    engine: &Engine,
    output: &Option<PathBuf>,
    format_arg: &Option<String>,
) -> Result<Format, i32> {
    if let Some(raw) = format_arg {
        return parse_format(raw).ok_or_else(|| {
            eprintln!("squash: unknown format '{raw}' (try zip, 7z, tar.gz, tar.zst, gz, xz, zst)");
            exit_codes::USAGE
        });
    }
    if let Some(out) = output {
        if let Some(f) = engine.registry().detect(out) {
            return Ok(f);
        }
    }
    Ok(Format::Zip)
}

/// Accept canonical names (`tar.gz`) and common extensions (`tgz`).
fn parse_format(raw: &str) -> Option<Format> {
    let raw = raw.to_ascii_lowercase();
    Format::ALL
        .into_iter()
        .find(|f| f.name() == raw || f.extensions().contains(&raw.as_str()))
}

fn default_output(inputs: &[PathBuf], format: Format) -> PathBuf {
    let first = &inputs[0];
    let name = first
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".to_string());
    let ext = format.extensions()[0];
    first
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{name}.{ext}"))
}

/// Stream a job's events to the user and return the process exit code.
fn drive(handle: &JobHandle, job: &Job, json: bool, archive: Option<&Path>) -> i32 {
    if json {
        print_json(&JobLine::new(job, handle.id().0));
    }
    let mut bar = ProgressBar::hidden();
    let mut result: Option<Result<JobStats, SquashError>> = None;
    while let Some(event) = handle.next_event() {
        match event {
            ProgressEvent::Started {
                total_bytes_estimate,
            } => {
                if json {
                    print_json(&StartedLine::new(total_bytes_estimate));
                } else {
                    bar = progress_bar(total_bytes_estimate);
                }
            }
            ProgressEvent::Advanced {
                bytes_done,
                entries_done,
                current_path,
            } => {
                if json {
                    print_json(&ProgressLine::new(bytes_done, entries_done, &current_path));
                } else {
                    bar.set_position(bytes_done);
                    bar.set_message(current_path.display().to_string());
                }
            }
            ProgressEvent::Finished { stats } => result = Some(Ok(stats)),
            ProgressEvent::Failed { error } => result = Some(Err(error)),
        }
    }
    bar.finish_and_clear();
    match result {
        Some(Ok(stats)) => {
            if json {
                print_json(&ResultLine::finished(&stats));
            } else {
                println!("{}", summary_line(job, archive, &stats));
            }
            exit_codes::SUCCESS
        }
        Some(Err(error)) => {
            if json {
                print_json(&ResultLine::failed(error.clone()));
            }
            eprintln!("squash: {}", plain_message(&error, archive));
            error_exit_code(&error)
        }
        None => fail(json, "internal error", exit_codes::INTERNAL),
    }
}

fn progress_bar(total: Option<u64>) -> ProgressBar {
    let bar = match total {
        Some(bytes) => ProgressBar::new(bytes),
        None => ProgressBar::new_spinner(),
    };
    bar.set_draw_target(ProgressDrawTarget::stderr());
    bar.set_style(
        ProgressStyle::with_template(
            "{percent:>3}% {bar:30} {bytes}/{total_bytes} ETA {eta} {msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    bar
}

/// The S4 summary line (docs/03): `before → after` plus savings for compress.
fn summary_line(job: &Job, archive: Option<&Path>, stats: &JobStats) -> String {
    match job.operation {
        Operation::Compress => {
            let saved = if stats.in_bytes > 0 {
                100.0 - (stats.out_bytes as f64 / stats.in_bytes as f64) * 100.0
            } else {
                0.0
            };
            let verdict = if saved >= 0.0 {
                format!("saved {saved:.0}%")
            } else {
                format!("grew {:.0}%", -saved)
            };
            format!(
                "{} — {} → {}, {}",
                job.destination.display(),
                fmt_bytes(stats.in_bytes),
                fmt_bytes(stats.out_bytes),
                verdict
            )
        }
        Operation::Extract => {
            let name = archive
                .map(|a| a.display().to_string())
                .unwrap_or_else(|| "archive".to_string());
            format!(
                "{} — {} → {} (extracted to {})",
                name,
                fmt_bytes(stats.in_bytes),
                fmt_bytes(stats.out_bytes),
                job.destination.display()
            )
        }
    }
}

/// Plain-language error strings (docs/03 F7 wording; they move behind i18n
/// keys with the GUI's string table).
fn plain_message(error: &SquashError, archive: Option<&Path>) -> String {
    match error {
        SquashError::UnsupportedFormat => "unsupported format".to_string(),
        SquashError::CorruptArchive => format!(
            "corrupt archive: {}",
            archive.map(|a| a.display().to_string()).unwrap_or_default()
        ),
        SquashError::PathTraversalBlocked => {
            "this archive tried to write outside the destination folder and was blocked".to_string()
        }
        SquashError::PermissionDenied => "permission denied".to_string(),
        SquashError::DiskFull => "disk full".to_string(),
        SquashError::PasswordRequired => {
            "this archive is encrypted. Squash v1 can't open encrypted archives".to_string()
        }
        SquashError::Cancelled => "cancelled".to_string(),
        SquashError::Internal => "internal error".to_string(),
    }
}

/// `SquashError` → deterministic exit code (docs/03 §4).
pub fn error_exit_code(error: &SquashError) -> i32 {
    match error {
        SquashError::UnsupportedFormat => exit_codes::USAGE,
        SquashError::CorruptArchive => exit_codes::CORRUPT,
        SquashError::PasswordRequired => exit_codes::ENCRYPTED,
        SquashError::PermissionDenied | SquashError::DiskFull => exit_codes::IO,
        SquashError::PathTraversalBlocked => exit_codes::UNSAFE_PATH,
        SquashError::Cancelled => exit_codes::CANCELLED,
        SquashError::Internal => exit_codes::INTERNAL,
    }
}

/// Non-job failure (usage errors): human message on stderr, nothing on
/// stdout so `--json` consumers only ever see the documented schema.
fn fail(json: bool, message: &str, code: i32) -> i32 {
    let _ = json;
    eprintln!("squash: {message}");
    code
}

fn print_json<T: Serialize>(line: &T) {
    match serde_json::to_string(line) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("squash: failed to serialize json output: {e}"),
    }
}

/// Binary-unit byte formatting for the human summary line.
pub fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

// --- --json projection (the documented JSONL schema) -------------------------

#[derive(Serialize)]
struct JobLine<'a> {
    r#type: &'static str,
    id: u64,
    operation: Operation,
    inputs: &'a [PathBuf],
    destination: &'a Path,
    format: Format,
    preset: Preset,
}

impl<'a> JobLine<'a> {
    fn new(job: &'a Job, id: u64) -> Self {
        Self {
            r#type: "job",
            id,
            operation: job.operation,
            inputs: &job.inputs,
            destination: &job.destination,
            format: job.format,
            preset: job.preset,
        }
    }
}

#[derive(Serialize)]
struct StartedLine {
    r#type: &'static str,
    total_bytes_estimate: Option<u64>,
}

impl StartedLine {
    fn new(total_bytes_estimate: Option<u64>) -> Self {
        Self {
            r#type: "started",
            total_bytes_estimate,
        }
    }
}

#[derive(Serialize)]
struct ProgressLine<'a> {
    r#type: &'static str,
    bytes_done: u64,
    entries_done: u64,
    current_path: &'a Path,
}

impl<'a> ProgressLine<'a> {
    fn new(bytes_done: u64, entries_done: u64, current_path: &'a Path) -> Self {
        Self {
            r#type: "progress",
            bytes_done,
            entries_done,
            current_path,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum ResultLine {
    Finished {
        r#type: &'static str,
        in_bytes: u64,
        out_bytes: u64,
        duration_ms: u64,
    },
    Failed {
        r#type: &'static str,
        error: SquashError,
    },
}

impl ResultLine {
    fn finished(stats: &JobStats) -> Self {
        Self::Finished {
            r#type: "result",
            in_bytes: stats.in_bytes,
            out_bytes: stats.out_bytes,
            duration_ms: stats.duration.as_millis() as u64,
        }
    }

    fn failed(error: SquashError) -> Self {
        Self::Failed {
            r#type: "result",
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_parsing_accepts_names_and_extensions() {
        assert_eq!(parse_format("zip"), Some(Format::Zip));
        assert_eq!(parse_format("tar.gz"), Some(Format::TarGz));
        assert_eq!(parse_format("TGZ"), Some(Format::TarGz));
        assert_eq!(parse_format("tar.zst"), Some(Format::TarZst));
        assert_eq!(parse_format("rar"), Some(Format::Rar));
        assert_eq!(parse_format("nope"), None);
    }

    #[test]
    fn default_output_sits_next_to_first_input() {
        assert_eq!(
            default_output(&[PathBuf::from("/a/b/src")], Format::Zip),
            PathBuf::from("/a/b/src.zip")
        );
        assert_eq!(
            default_output(&[PathBuf::from("docs")], Format::TarGz),
            PathBuf::from("docs.tar.gz")
        );
    }

    #[test]
    fn error_codes_follow_the_documented_mapping() {
        assert_eq!(
            error_exit_code(&SquashError::CorruptArchive),
            exit_codes::CORRUPT
        );
        assert_eq!(
            error_exit_code(&SquashError::PathTraversalBlocked),
            exit_codes::UNSAFE_PATH
        );
        assert_eq!(
            error_exit_code(&SquashError::PasswordRequired),
            exit_codes::ENCRYPTED
        );
        assert_eq!(error_exit_code(&SquashError::DiskFull), exit_codes::IO);
        assert_eq!(
            error_exit_code(&SquashError::UnsupportedFormat),
            exit_codes::USAGE
        );
        assert_eq!(
            error_exit_code(&SquashError::Cancelled),
            exit_codes::CANCELLED
        );
    }

    #[test]
    fn json_result_lines_match_schema() {
        let line = serde_json::to_string(&ResultLine::failed(SquashError::CorruptArchive)).unwrap();
        assert_eq!(
            line,
            r#"{"status":"failed","type":"result","error":"corrupt_archive"}"#
        );
    }

    #[test]
    fn fmt_bytes_uses_binary_units() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(2048), "2.0 KiB");
        assert_eq!(fmt_bytes(5 * 1024 * 1024), "5.0 MiB");
    }
}
