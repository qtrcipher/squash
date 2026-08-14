//! 7z handler — create + extract via `sevenz-rust2` (docs/05 §4).
//!
//! Compression is LZMA2, non-solid, level from the preset table (1/5/9) via
//! [`super::clamped_level`]. Non-solid keeps per-entry progress and
//! cancellation exact; the ratio cost vs solid is accepted at this stage.
//!
//! Extraction goes entry-by-entry through [`crate::safety`] like every other
//! handler: names are normalized to `/` separators (some tools write `\`),
//! anti-items (7z update-deletion markers) are never materialized, and
//! encrypted archives abort with [`SquashError::PasswordRequired`] —
//! encryption is out of scope for v1 (docs/03 D2).
//!
//! Known limitation: `sevenz-rust2` has no symlink API, so symlinks are
//! **dereferenced** on compress (the target's contents are stored as a plain
//! file) — the same fallback the zip handler uses on non-unix.

use super::{map_decode, map_io, slash_path, walk_inputs};
use crate::error::SquashError;
use crate::format::{Format, FormatHandler, HandlerContext};
use crate::job::Job;
use crate::layout::{extraction_target, EntryMeta};
use crate::progress::{JobStats, ProgressEvent};
use crate::safety::sanitize_entry_path;
use sevenz_rust2::{ArchiveEntry, ArchiveReader, ArchiveWriter, Error as SevenZError, Password};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct SevenZHandler;

impl FormatHandler for SevenZHandler {
    fn format(&self) -> Format {
        Format::SevenZ
    }

    fn can_extract(&self) -> bool {
        true
    }

    fn can_create(&self) -> bool {
        true
    }

    fn create(&self, job: &Job, ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        let start = Instant::now();
        let level = super::clamped_level(Format::SevenZ, job.preset)?;
        // Walk first: a bad input must not leave an empty archive behind.
        let entries = walk_inputs(&job.inputs)?;
        log::debug!(
            "7z create: {} entries → {} (level {level})",
            entries.len(),
            job.destination.display()
        );

        let file = File::create(&job.destination).map_err(map_io)?;
        let mut writer = ArchiveWriter::new(file).map_err(map_7z_write)?;
        writer.set_content_methods(vec![
            sevenz_rust2::encoder_options::Lzma2Options::from_level(u32::from(level)).into(),
        ]);

        let mut in_bytes = 0u64;
        for (index, entry) in entries.iter().enumerate() {
            ctx.check_cancelled()?;
            let name = slash_path(&entry.archive_path);
            if entry.is_dir {
                writer
                    .push_archive_entry(ArchiveEntry::new_directory(&name), None::<&[u8]>)
                    .map_err(map_7z_write)?;
            } else {
                // `File::open` follows symlinks: no symlink API in the crate
                // (see module docs), so link targets are stored as files.
                let input = File::open(&entry.fs_path).map_err(map_io)?;
                writer
                    .push_archive_entry(ArchiveEntry::new_file(&name), Some(input))
                    .map_err(map_7z_write)?;
                in_bytes += entry.len;
            }
            ctx.report(ProgressEvent::Advanced {
                bytes_done: in_bytes,
                entries_done: index as u64 + 1,
                current_path: entry.fs_path.clone(),
            });
        }
        writer.finish().map_err(map_io)?;

        Ok(JobStats {
            in_bytes,
            out_bytes: fs::metadata(&job.destination).map(|m| m.len()).unwrap_or(0),
            duration: start.elapsed(),
        })
    }

    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        let start = Instant::now();
        let file = File::open(archive).map_err(map_io)?;
        let mut reader = ArchiveReader::new(file, Password::empty()).map_err(map_7z_decode)?;
        log::debug!("7z extract: {} → {}", archive.display(), dest_dir.display());

        // Metadata pass drives the docs/03 F3 layout decision before anything
        // is written.
        let meta: Vec<EntryMeta> = reader
            .archive()
            .files
            .iter()
            .filter(|f| !f.is_anti_item)
            .map(|f| EntryMeta {
                path: entry_path(f.name()),
                is_dir: f.is_directory(),
            })
            .collect();
        let target = extraction_target(dest_dir, archive, Format::SevenZ, &meta);
        fs::create_dir_all(&target).map_err(map_io)?;

        // The closure speaks the crate's error type; our own failures travel
        // out through this slot and are checked after the decode loop.
        let mut inner: Option<SquashError> = None;
        let mut out_bytes = 0u64;
        let mut entries_done = 0u64;
        let decode = reader.for_each_entries(|entry, data| {
            if inner.is_some() {
                return Ok(false);
            }
            match extract_entry(entry, data, &target, ctx, &mut out_bytes) {
                // Anti-items are skipped silently: no entry, no progress.
                Ok(None) => Ok(true),
                Ok(Some(path)) => {
                    entries_done += 1;
                    ctx.report(ProgressEvent::Advanced {
                        bytes_done: out_bytes,
                        entries_done,
                        current_path: path,
                    });
                    Ok(true)
                }
                Err(err) => {
                    inner = Some(err);
                    Ok(false)
                }
            }
        });
        if let Some(err) = inner {
            return Err(err);
        }
        decode.map_err(map_7z_decode)?;

        Ok(JobStats {
            in_bytes: fs::metadata(archive).map(|m| m.len()).unwrap_or(0),
            out_bytes,
            duration: start.elapsed(),
        })
    }
}

/// Extract one entry under the (already layout-resolved) target. The path is
/// sanitized before a single byte is written; reads map to corrupt, writes
/// to the I/O taxonomy. Returns the on-disk path for progress reporting, or
/// `None` for skipped anti-items.
fn extract_entry(
    entry: &ArchiveEntry,
    data: &mut dyn Read,
    target: &Path,
    ctx: &HandlerContext,
    out_bytes: &mut u64,
) -> Result<Option<PathBuf>, SquashError> {
    ctx.check_cancelled()?;
    if entry.is_anti_item {
        return Ok(None);
    }
    let path = sanitize_entry_path(target, &entry_path(entry.name()))?;
    if entry.is_directory() {
        fs::create_dir_all(&path).map_err(map_io)?;
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(map_io)?;
        }
        let mut out = File::create(&path).map_err(map_io)?;
        copy_entry(data, &mut out)?;
        *out_bytes += entry.size();
    }
    Ok(Some(path))
}

/// Split-side copy (see `tar_family::copy_entry`): read errors come from the
/// decode stream (→ corrupt), write errors from the disk (→ io mapping).
fn copy_entry(reader: &mut dyn Read, writer: &mut impl Write) -> Result<(), SquashError> {
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(map_decode)?;
        if n == 0 {
            return Ok(());
        }
        writer.write_all(&buf[..n]).map_err(map_io)?;
    }
}

/// Entry name normalized to forward slashes (some tools write `\`).
fn entry_path(name: &str) -> PathBuf {
    PathBuf::from(name.replace('\\', "/"))
}

/// Decode-position errors: everything structural is a corrupt archive,
/// encrypted content is `PasswordRequired` (out of scope v1, docs/03 D2),
/// and real I/O kinds still route through the decode/io split.
fn map_7z_decode(err: SevenZError) -> SquashError {
    match err {
        SevenZError::PasswordRequired | SevenZError::MaybeBadPassword(_) => {
            SquashError::PasswordRequired
        }
        SevenZError::Io(e, _) | SevenZError::FileOpen(e, _) => map_decode(e),
        _ => SquashError::CorruptArchive,
    }
}

/// Encode-position errors: disk problems map through the io taxonomy,
/// everything else is an internal failure of the writer itself.
fn map_7z_write(err: SevenZError) -> SquashError {
    match err {
        SevenZError::Io(e, _) | SevenZError::FileOpen(e, _) => map_io(e),
        _ => SquashError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_path_normalizes_backslashes() {
        assert_eq!(entry_path("a\\b\\c.txt"), PathBuf::from("a/b/c.txt"));
        assert_eq!(entry_path("مجلد/ملف.txt"), PathBuf::from("مجلد/ملف.txt"));
    }
}
