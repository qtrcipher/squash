//! zip handler — create + extract via the `zip` crate (docs/05 §4).
//!
//! Security posture: extraction goes entry-by-entry through
//! [`crate::safety`] (no `ZipArchive::extract`, ever), symlinks are validated
//! before creation, and encrypted entries abort with
//! [`SquashError::PasswordRequired`] — encryption is out of scope for v1
//! (docs/03 D2).

use super::{copy_guarded, map_io, map_transfer, slash_path, walk_inputs};
use crate::error::SquashError;
use crate::format::{Format, FormatHandler, HandlerContext};
use crate::job::Job;
use crate::layout::{extraction_target, EntryMeta};
use crate::progress::{JobStats, ProgressEvent};
use crate::safety::{
    create_dir_all_guarded, create_file_guarded, sanitize_entry_path, sanitize_link_target,
    ExtractGuard,
};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub struct ZipHandler;

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;
/// A symlink target longer than this is not a link, it's an attack or a bug
/// (PATH_MAX is 4096 on Linux, ~1024 on macOS) — refuse before the unbounded
/// `read_to_string` allocates on attacker-controlled entry data.
const MAX_SYMLINK_TARGET: u64 = 8192;

impl FormatHandler for ZipHandler {
    fn format(&self) -> Format {
        Format::Zip
    }

    fn can_extract(&self) -> bool {
        true
    }

    fn can_create(&self) -> bool {
        true
    }

    fn create(&self, job: &Job, ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        let start = Instant::now();
        let level = super::clamped_level(Format::Zip, job.preset)?;
        // Walk first: a bad input must not leave an empty archive behind.
        let entries = walk_inputs(&job.inputs)?;
        log::debug!(
            "zip create: {} entries → {} (level {level})",
            entries.len(),
            job.destination.display()
        );

        let file = File::create(&job.destination).map_err(map_io)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(i64::from(level)));

        let mut in_bytes = 0u64;
        for (index, entry) in entries.iter().enumerate() {
            ctx.check_cancelled()?;
            let name = slash_path(&entry.archive_path);
            if entry.is_dir {
                zip.add_directory(&name, options).map_err(map_zip)?;
            } else if entry.fs_path.is_symlink() {
                #[cfg(unix)]
                {
                    let target = fs::read_link(&entry.fs_path).map_err(map_io)?;
                    zip.add_symlink(&name, target.to_string_lossy(), options)
                        .map_err(map_zip)?;
                }
                #[cfg(not(unix))]
                {
                    // No portable symlink creation: store the link target's
                    // contents as a plain file.
                    let mut input = File::open(&entry.fs_path).map_err(map_io)?;
                    zip.start_file(&name, options).map_err(map_zip)?;
                    in_bytes += io::copy(&mut input, &mut zip).map_err(map_io)?;
                }
            } else {
                let mut input = File::open(&entry.fs_path).map_err(map_io)?;
                zip.start_file(&name, options).map_err(map_zip)?;
                io::copy(&mut input, &mut zip).map_err(map_io)?;
                in_bytes += entry.len;
            }
            ctx.report(ProgressEvent::Advanced {
                bytes_done: in_bytes,
                entries_done: index as u64 + 1,
                current_path: entry.fs_path.clone(),
            });
        }
        zip.finish().map_err(map_zip)?;

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
        let mut zip = ZipArchive::new(file).map_err(map_zip)?;
        log::debug!(
            "zip extract: {} ({} entries) → {}",
            archive.display(),
            zip.len(),
            dest_dir.display()
        );

        // Pass 1: entry metadata drives the docs/03 F3 layout decision and
        // rejects encrypted archives before anything is written.
        let mut meta = Vec::with_capacity(zip.len());
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(map_zip)?;
            if entry.encrypted() {
                return Err(SquashError::PasswordRequired);
            }
            meta.push(EntryMeta {
                path: entry_path(&entry),
                is_dir: entry.is_dir(),
            });
        }
        let target = extraction_target(dest_dir, archive, Format::Zip, &meta);
        let in_bytes = fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
        let mut guard = ExtractGuard::new(in_bytes);
        // Track the layout folder only when we actually create it — never
        // mark a pre-existing destination for rollback.
        let fresh_target = target != dest_dir && !target.exists();
        fs::create_dir_all(&target).map_err(map_io)?;
        if fresh_target {
            guard.track_created(target.clone());
        }

        // Pass 2: sanitized, entry-by-entry extraction. Any failure rolls
        // back the partial output the job created.
        let result = self.extract_entries(&mut zip, &target, ctx, &mut guard);
        match result {
            Ok(()) => {}
            Err(err) => {
                guard.rollback();
                return Err(err);
            }
        }

        Ok(JobStats {
            in_bytes,
            out_bytes: guard.out_bytes(),
            duration: start.elapsed(),
        })
    }
}

impl ZipHandler {
    /// Pass 2 of extraction: write every entry under `target`, counting
    /// actual bytes against the bomb guard.
    fn extract_entries<R: io::Read + io::Seek>(
        &self,
        zip: &mut ZipArchive<R>,
        target: &Path,
        ctx: &HandlerContext,
        guard: &mut ExtractGuard,
    ) -> Result<(), SquashError> {
        for i in 0..zip.len() {
            ctx.check_cancelled()?;
            guard.record_entry()?;
            let mut entry = zip.by_index(i).map_err(map_zip)?;
            let path = sanitize_entry_path(target, &entry_path(&entry))?;
            let is_symlink = entry
                .unix_mode()
                .map(|mode| mode & S_IFMT == S_IFLNK)
                .unwrap_or(false);

            if entry.is_dir() {
                create_dir_all_guarded(target, &path, guard)?;
            } else if is_symlink {
                let mut link_target = String::new();
                entry
                    .by_ref()
                    .take(MAX_SYMLINK_TARGET + 1)
                    .read_to_string(&mut link_target)
                    .map_err(map_transfer)?;
                if link_target.len() as u64 > MAX_SYMLINK_TARGET {
                    return Err(SquashError::CorruptArchive);
                }
                // Parents first so target validation resolves against the
                // real tree; the job aborts on escape.
                if let Some(parent) = path.parent() {
                    create_dir_all_guarded(target, parent, guard)?;
                }
                sanitize_link_target(target, &path, Path::new(&link_target))?;
                #[cfg(unix)]
                {
                    let fresh = !path.exists();
                    let _ = fs::remove_file(&path);
                    std::os::unix::fs::symlink(&link_target, &path).map_err(map_io)?;
                    if fresh {
                        guard.track_created(path.clone());
                    }
                }
                // Non-unix: symlink materialization is skipped (Windows needs
                // privileges); the target was still validated above.
            } else {
                let mut out = create_file_guarded(target, &path, guard)?;
                copy_guarded(&mut entry, &mut out, guard, map_transfer)?;
            }
            ctx.report(ProgressEvent::Advanced {
                bytes_done: guard.out_bytes(),
                entries_done: i as u64 + 1,
                current_path: path,
            });
        }
        Ok(())
    }
}

/// Entry name normalized to forward slashes (some tools write `\`).
fn entry_path<R: io::Read>(entry: &zip::read::ZipFile<'_, R>) -> PathBuf {
    PathBuf::from(entry.name().replace('\\', "/"))
}

fn map_zip(err: zip::result::ZipError) -> SquashError {
    use zip::result::ZipError as Z;
    match err {
        Z::Io(e) => map_io(e),
        Z::InvalidArchive(_) | Z::UnsupportedArchive(_) => SquashError::CorruptArchive,
        Z::InvalidPassword => SquashError::PasswordRequired,
        _ => SquashError::Internal,
    }
}
