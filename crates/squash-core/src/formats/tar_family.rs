//! tar-family handlers (docs/05 §4): `tar.gz`/`tar.zst` create+extract,
//! `tar`/`tar.bz2`/`tar.xz` extract-only.
//!
//! Extraction never uses `tar::Archive::unpack`: entries are walked one by
//! one through the [`crate::safety`] layer, device nodes/fifos are never
//! materialized, and link targets are validated before link creation.
//!
//! Extraction is two-pass by design: pass 1 reads entry metadata to make the
//! docs/03 F3 layout decision (single-root vs loose), pass 2 writes. That
//! costs a second decode of the stream; correctness of the layout rule beats
//! the saving at this stage.

use super::{map_decode, map_io, walk_inputs, WalkEntry};
use crate::error::SquashError;
use crate::format::{Format, FormatHandler, HandlerContext};
use crate::job::Job;
use crate::layout::{extraction_target, EntryMeta};
use crate::progress::{JobStats, ProgressEvent};
use crate::safety::{sanitize_entry_path, sanitize_link_target};
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use std::time::Instant;
use tar::EntryType;

pub struct TarHandler;
pub struct TarGzHandler;
pub struct TarBz2Handler;
pub struct TarXzHandler;
pub struct TarZstHandler;

impl FormatHandler for TarHandler {
    fn format(&self) -> Format {
        Format::Tar
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        false
    }
    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        extract_with(archive, dest_dir, Format::Tar, ctx, |file| {
            Ok(Box::new(BufReader::new(file)))
        })
    }
}

impl FormatHandler for TarGzHandler {
    fn format(&self) -> Format {
        Format::TarGz
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        true
    }
    fn create(&self, job: &Job, ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        let level = super::clamped_level(Format::TarGz, job.preset)?;
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
        extract_with(archive, dest_dir, Format::TarGz, ctx, |file| {
            Ok(Box::new(flate2::read::GzDecoder::new(BufReader::new(file))))
        })
    }
}

impl FormatHandler for TarBz2Handler {
    fn format(&self) -> Format {
        Format::TarBz2
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        false
    }
    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        extract_with(archive, dest_dir, Format::TarBz2, ctx, |file| {
            Ok(Box::new(bzip2::read::BzDecoder::new(BufReader::new(file))))
        })
    }
}

impl FormatHandler for TarXzHandler {
    fn format(&self) -> Format {
        Format::TarXz
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        false
    }
    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        extract_with(archive, dest_dir, Format::TarXz, ctx, |file| {
            Ok(Box::new(xz2::read::XzDecoder::new(BufReader::new(file))))
        })
    }
}

impl FormatHandler for TarZstHandler {
    fn format(&self) -> Format {
        Format::TarZst
    }
    fn can_extract(&self) -> bool {
        true
    }
    fn can_create(&self) -> bool {
        true
    }
    fn create(&self, job: &Job, ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        let level = super::clamped_level(Format::TarZst, job.preset)?;
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
        extract_with(archive, dest_dir, Format::TarZst, ctx, |file| {
            zstd::stream::read::Decoder::new(BufReader::new(file))
                .map(|d| Box::new(d) as Box<dyn Read>)
        })
    }
}

// --- create -----------------------------------------------------------------

/// Shared tar-family create: walk inputs, stream them through the encoder,
/// then finish the encoder explicitly (drop-finish would swallow errors).
fn create_with<E: Write>(
    job: &Job,
    ctx: &HandlerContext,
    make_encoder: impl FnOnce(File) -> Result<E, SquashError>,
    finish: impl FnOnce(E) -> Result<(), SquashError>,
) -> Result<JobStats, SquashError> {
    let start = Instant::now();
    // Walk first: a bad input must not leave an empty archive behind.
    let entries = walk_inputs(&job.inputs)?;
    let file = File::create(&job.destination).map_err(map_io)?;
    let encoder = make_encoder(file)?;
    let (encoder, in_bytes) = build_tar(encoder, &entries, ctx)?;
    finish(encoder)?;
    Ok(JobStats {
        in_bytes,
        out_bytes: fs::metadata(&job.destination).map(|m| m.len()).unwrap_or(0),
        duration: start.elapsed(),
    })
}

fn build_tar<W: Write>(
    writer: W,
    entries: &[WalkEntry],
    ctx: &HandlerContext,
) -> Result<(W, u64), SquashError> {
    let mut builder = tar::Builder::new(writer);
    builder.follow_symlinks(false);
    let mut in_bytes = 0u64;
    for (index, entry) in entries.iter().enumerate() {
        ctx.check_cancelled()?;
        builder
            .append_path_with_name(&entry.fs_path, &entry.archive_path)
            .map_err(map_io)?;
        in_bytes += entry.len;
        ctx.report(ProgressEvent::Advanced {
            bytes_done: in_bytes,
            entries_done: index as u64 + 1,
            current_path: entry.fs_path.clone(),
        });
    }
    // `into_inner` writes the trailing tar blocks and returns the encoder.
    let writer = builder.into_inner().map_err(map_io)?;
    Ok((writer, in_bytes))
}

// --- extract ----------------------------------------------------------------

fn extract_with(
    archive: &Path,
    dest_dir: &Path,
    format: Format,
    ctx: &HandlerContext,
    decoder: impl Fn(File) -> io::Result<Box<dyn Read>>,
) -> Result<JobStats, SquashError> {
    let start = Instant::now();
    let open = || -> Result<Box<dyn Read>, SquashError> {
        let file = File::open(archive).map_err(map_io)?;
        decoder(file).map_err(map_decode)
    };
    // Pass 1: metadata for the layout decision.
    let meta = tar_list(open()?)?;
    let target = extraction_target(dest_dir, archive, format, &meta);
    // Pass 2: sanitized extraction.
    let out_bytes = tar_extract(open()?, &target, ctx)?;
    Ok(JobStats {
        in_bytes: fs::metadata(archive).map(|m| m.len()).unwrap_or(0),
        out_bytes,
        duration: start.elapsed(),
    })
}

fn tar_list<R: Read>(reader: R) -> Result<Vec<EntryMeta>, SquashError> {
    let mut archive = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in archive.entries().map_err(map_decode)? {
        let entry = entry.map_err(map_decode)?;
        out.push(EntryMeta {
            path: entry.path().map_err(map_decode)?.into_owned(),
            is_dir: entry.header().entry_type() == EntryType::Directory,
        });
    }
    Ok(out)
}

/// Copy an entry's payload to disk with the error sides split: read errors
/// come from the decode stream (→ corrupt), write errors from the disk
/// (→ io mapping). `io::copy` cannot tell the two apart.
fn copy_entry<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> Result<u64, SquashError> {
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(map_decode)?;
        if n == 0 {
            return Ok(total);
        }
        writer.write_all(&buf[..n]).map_err(map_io)?;
        total += n as u64;
    }
}

fn tar_extract<R: Read>(
    reader: R,
    target: &Path,
    ctx: &HandlerContext,
) -> Result<u64, SquashError> {
    let mut archive = tar::Archive::new(reader);
    fs::create_dir_all(target).map_err(map_io)?;
    let mut out_bytes = 0u64;
    for (index, entry) in archive.entries().map_err(map_decode)?.enumerate() {
        ctx.check_cancelled()?;
        let mut entry = entry.map_err(map_decode)?;
        let path = sanitize_entry_path(target, &entry.path().map_err(map_decode)?)?;
        match entry.header().entry_type() {
            EntryType::Directory => {
                fs::create_dir_all(&path).map_err(map_io)?;
            }
            EntryType::Regular | EntryType::Continuous => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(map_io)?;
                }
                let mut out = File::create(&path).map_err(map_io)?;
                copy_entry(&mut entry, &mut out)?;
                out_bytes += entry.header().size().map_err(map_decode)?;
            }
            EntryType::Symlink => {
                let link_target = entry
                    .link_name()
                    .map_err(map_decode)?
                    .ok_or(SquashError::CorruptArchive)?
                    .into_owned();
                // Validate BEFORE creating: the job aborts on escape.
                sanitize_link_target(target, &path, &link_target)?;
                #[cfg(unix)]
                {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).map_err(map_io)?;
                    }
                    let _ = fs::remove_file(&path);
                    std::os::unix::fs::symlink(&link_target, &path).map_err(map_io)?;
                }
                // Non-unix: skipped after validation (see zip handler note).
            }
            EntryType::Link => {
                // Hardlink targets are archive-root-relative paths.
                let link = entry
                    .link_name()
                    .map_err(map_decode)?
                    .ok_or(SquashError::CorruptArchive)?;
                let source = sanitize_entry_path(target, &link)?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(map_io)?;
                }
                fs::hard_link(&source, &path).map_err(map_io)?;
            }
            // Char/block devices, fifos, GNU sparse payloads: never
            // materialize special files from an archive.
            _ => {}
        }
        ctx.report(ProgressEvent::Advanced {
            bytes_done: out_bytes,
            entries_done: index as u64 + 1,
            current_path: path,
        });
    }
    Ok(out_bytes)
}
