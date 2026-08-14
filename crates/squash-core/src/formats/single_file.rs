//! Single-file codec handlers (docs/05 §4): `gz` / `xz` / `zst`.
//!
//! These are *codecs, not archivers*: one stream in, one stream out, no
//! container metadata. Three thin handlers share one generic implementation
//! (mirroring the tar-family split) because the only per-codec difference is
//! the encoder/decoder pair.
//!
//! Semantics:
//! - **Compress** takes exactly ONE regular-file input; a directory or
//!   multiple inputs is a usage error ([`SquashError::UnsupportedFormat`] →
//!   CLI exit 2 — the CLI pre-validates with a "use tar.gz for directories"
//!   hint). Output is one compressed stream; `<name>.<ext>` naming and the
//!   docs/03 F4 one-output-per-input batch split are the CLI's job.
//! - **Extract** writes `<dest>/<archive-name minus ONE codec extension>`
//!   (`data.csv.gz` → `data.csv`). The docs/03 F3 single-root-vs-loose rule
//!   is for multi-file archives and never applies here: no folder is created
//!   for the payload.
//! - Both directions stream in 64 KiB chunks; nothing is buffered whole.
//!   Decompression-bomb guarding (ratio/size caps) is Phase 3 — the decode
//!   loop below is deliberately unbounded for now.

use super::{map_decode, map_io};
use crate::error::SquashError;
use crate::format::{Format, FormatHandler, HandlerContext};
use crate::job::Job;
use crate::layout::archive_stem;
use crate::progress::{JobStats, ProgressEvent};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct GzHandler;
pub struct XzHandler;
pub struct ZstHandler;

impl FormatHandler for GzHandler {
    fn format(&self) -> Format {
        Format::Gz
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        true
    }
    fn create(&self, job: &Job, ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        let level = super::clamped_level(Format::Gz, job.preset)?;
        create_with(
            job,
            ctx,
            |file| {
                Ok(flate2::write::GzEncoder::new(
                    file,
                    flate2::Compression::new(u32::from(level)),
                ))
            },
            |enc| enc.finish().map(|_| ()).map_err(map_io),
        )
    }
    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        extract_with(archive, dest_dir, Format::Gz, ctx, |file| {
            // Multi-member: concatenated gzip members decode as one stream,
            // like the gzip CLI.
            Ok(Box::new(flate2::read::MultiGzDecoder::new(BufReader::new(
                file,
            ))))
        })
    }
}

impl FormatHandler for XzHandler {
    fn format(&self) -> Format {
        Format::Xz
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        true
    }
    fn create(&self, job: &Job, ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        let level = super::clamped_level(Format::Xz, job.preset)?;
        create_with(
            job,
            ctx,
            |file| Ok(xz2::write::XzEncoder::new(file, u32::from(level))),
            |enc| enc.finish().map(|_| ()).map_err(map_io),
        )
    }
    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        extract_with(archive, dest_dir, Format::Xz, ctx, |file| {
            Ok(Box::new(xz2::read::XzDecoder::new(BufReader::new(file))))
        })
    }
}

impl FormatHandler for ZstHandler {
    fn format(&self) -> Format {
        Format::Zst
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        true
    }
    fn create(&self, job: &Job, ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        let level = super::clamped_level(Format::Zst, job.preset)?;
        create_with(
            job,
            ctx,
            |file| zstd::stream::write::Encoder::new(file, i32::from(level)).map_err(map_io),
            |enc| enc.finish().map(|_| ()).map_err(map_io),
        )
    }
    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        extract_with(archive, dest_dir, Format::Zst, ctx, |file| {
            zstd::stream::read::Decoder::new(BufReader::new(file))
                .map(|d| Box::new(d) as Box<dyn Read>)
        })
    }
}

/// The usage contract: exactly one input, and it must be a regular file.
/// `UnsupportedFormat` is the taxonomy's usage-error slot for a codec that
/// cannot express the request (the CLI maps it to exit 2 and pre-validates
/// with a friendlier "use tar.gz for directories" hint). A missing input
/// routes through [`map_io`] like the other handlers' walk failures.
fn single_file_input(inputs: &[PathBuf]) -> Result<&Path, SquashError> {
    if inputs.len() != 1 {
        return Err(SquashError::UnsupportedFormat);
    }
    let input = &inputs[0];
    let meta = fs::metadata(input).map_err(map_io)?;
    if !meta.is_file() {
        return Err(SquashError::UnsupportedFormat);
    }
    Ok(input)
}

/// Shared single-file create: validate the input contract, then stream the
/// file through the encoder and finish explicitly (drop-finish would swallow
/// errors). Validation happens BEFORE the output file exists so a rejected
/// job leaves nothing behind.
fn create_with<E: Write>(
    job: &Job,
    ctx: &HandlerContext,
    make_encoder: impl FnOnce(File) -> Result<E, SquashError>,
    finish: impl FnOnce(E) -> Result<(), SquashError>,
) -> Result<JobStats, SquashError> {
    let start = Instant::now();
    let input = single_file_input(&job.inputs)?;
    log::debug!(
        "{} create: {} → {}",
        job.format.name(),
        input.display(),
        job.destination.display()
    );
    let mut reader = BufReader::new(File::open(input).map_err(map_io)?);
    let file = File::create(&job.destination).map_err(map_io)?;
    let mut encoder = make_encoder(file)?;
    // Compress side: reads come from the disk, writes go into the encoder —
    // both map through the io mapping (a codec failure here is `Internal`).
    let in_bytes = pump(&mut reader, &mut encoder, input, ctx, map_io)?;
    finish(encoder)?;
    Ok(JobStats {
        in_bytes,
        out_bytes: fs::metadata(&job.destination).map(|m| m.len()).unwrap_or(0),
        duration: start.elapsed(),
    })
}

/// Shared single-file extract: decode the stream into
/// `<dest>/<name minus one codec extension>`, streaming with split read/write
/// error mapping (read = decode → corrupt, write = disk → io).
fn extract_with(
    archive: &Path,
    dest_dir: &Path,
    format: Format,
    ctx: &HandlerContext,
    decoder: impl FnOnce(File) -> io::Result<Box<dyn Read>>,
) -> Result<JobStats, SquashError> {
    let start = Instant::now();
    let file = File::open(archive).map_err(map_io)?;
    let mut reader = decoder(file).map_err(map_decode)?;
    // Strip exactly ONE codec extension (`data.csv.gz` → `data.csv`).
    let target = dest_dir.join(archive_stem(archive, format));
    log::debug!(
        "{} extract: {} → {}",
        format.name(),
        archive.display(),
        target.display()
    );
    // A name with no extension would target the archive itself — refuse
    // before `File::create` truncates the input we are reading.
    if target == archive {
        return Err(SquashError::Internal);
    }
    fs::create_dir_all(dest_dir).map_err(map_io)?;
    let mut out = BufWriter::new(File::create(&target).map_err(map_io)?);
    let out_bytes = pump(&mut reader, &mut out, &target, ctx, map_decode)?;
    out.flush().map_err(map_io)?;
    Ok(JobStats {
        in_bytes: fs::metadata(archive).map(|m| m.len()).unwrap_or(0),
        out_bytes,
        duration: start.elapsed(),
    })
}

/// Stream `reader` → `writer` in 64 KiB chunks with cancellation checks and
/// progress. Read errors are mapped by `map_read` (decode side on extract →
/// corrupt; disk side on compress → io), write errors always come from the
/// disk/encoder (→ io). Returns payload bytes moved.
fn pump<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    current_path: &Path,
    ctx: &HandlerContext,
    map_read: fn(io::Error) -> SquashError,
) -> Result<u64, SquashError> {
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        ctx.check_cancelled()?;
        let n = reader.read(&mut buf).map_err(map_read)?;
        if n == 0 {
            return Ok(total);
        }
        writer.write_all(&buf[..n]).map_err(map_io)?;
        total += n as u64;
        ctx.report(ProgressEvent::Advanced {
            bytes_done: total,
            entries_done: 1,
            current_path: current_path.to_path_buf(),
        });
    }
}
