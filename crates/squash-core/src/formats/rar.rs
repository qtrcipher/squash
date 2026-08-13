//! rar handler — **extraction only, forever** (docs/05 §4/§7): the RARLAB
//! UnRAR license permits handling RAR archives but forbids creating them.
//! Bytes come from the vendored UnRAR C++ source via `unrar-sys` (see
//! vendor/unrar/README.squash.md); this module is compiled only under the
//! `rar` cargo feature.
//!
//! Extraction mirrors the tar family: a metadata pass (RAR_OM_LIST) feeds the
//! docs/03 F3 layout decision, then a second open decodes entries. UnRAR runs
//! in `RAR_TEST` mode — it never touches the filesystem; every byte flows
//! through the entry callback into files Squash creates after
//! [`crate::safety::sanitize_entry_path`], exactly like the other handlers.
//!
//! Encrypted entries or encrypted headers abort with
//! [`SquashError::PasswordRequired`] (encryption is out of scope for v1,
//! docs/03 D2). Multi-volume archives abort as corrupt (the shim refuses the
//! volume-change callback).

use super::map_io;
use crate::error::SquashError;
use crate::format::{Format, FormatHandler, HandlerContext};
use crate::layout::{extraction_target, EntryMeta};
use crate::progress::{JobStats, ProgressEvent};
use crate::safety::sanitize_entry_path;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use unrar_sys::{RarArchive, RarError};

pub struct RarHandler;

impl FormatHandler for RarHandler {
    fn format(&self) -> Format {
        Format::Rar
    }

    fn can_extract(&self) -> bool {
        true
    }

    /// Never true: the RARLAB license forbids RAR creation (docs/05 §4).
    fn can_create(&self) -> bool {
        false
    }

    fn extract(
        &self,
        archive: &Path,
        dest_dir: &Path,
        ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        let start = Instant::now();
        // Open through the fs first so missing/unreadable archives get the
        // io taxonomy, identical to the other handlers.
        File::open(archive).map_err(map_io)?;

        // Pass 1: metadata for the layout decision. The DLL API requires a
        // RARProcessFile(RAR_SKIP) between header reads.
        let mut meta = Vec::new();
        {
            let mut reader = RarArchive::open_list(archive).map_err(map_rar)?;
            while let Some(entry) = reader.next_entry().map_err(map_rar)? {
                meta.push(EntryMeta {
                    path: entry_path(&entry.name),
                    is_dir: entry.is_dir,
                });
                reader.skip_current().map_err(map_rar)?;
            }
        }
        // UnRAR reports a garbage-after-signature file as an *empty* archive
        // (unknown blocks are skipped until EOF, no error) — and real `rar`
        // refuses to create empty archives, so zero entries means the input
        // is not a usable archive. Fail corrupt rather than report a
        // successful extraction of nothing.
        if meta.is_empty() {
            return Err(SquashError::CorruptArchive);
        }
        let target = extraction_target(dest_dir, archive, Format::Rar, &meta);
        fs::create_dir_all(&target).map_err(map_io)?;

        // Pass 2: sanitized extraction.
        let mut reader = RarArchive::open_extract(archive).map_err(map_rar)?;
        let mut out_bytes = 0u64;
        let mut entries_done = 0u64;
        while let Some(entry) = reader.next_entry().map_err(map_rar)? {
            ctx.check_cancelled()?;
            let path = sanitize_entry_path(&target, &entry_path(&entry.name))?;
            if entry.is_dir {
                fs::create_dir_all(&path).map_err(map_io)?;
                reader.skip_current().map_err(map_rar)?;
            } else {
                if entry.is_encrypted {
                    return Err(SquashError::PasswordRequired);
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(map_io)?;
                }
                let mut out = File::create(&path).map_err(map_io)?;
                // Write failures travel out through this slot; the callback
                // aborts the decode and `ABORTED` re-raises the stored error.
                let mut inner: Option<SquashError> = None;
                let decode = reader.extract_current(&mut |chunk: &[u8]| {
                    if inner.is_some() {
                        return false;
                    }
                    match out.write_all(chunk) {
                        Ok(()) => true,
                        Err(err) => {
                            inner = Some(map_io(err));
                            false
                        }
                    }
                });
                if let Some(err) = inner {
                    return Err(err);
                }
                decode.map_err(map_rar)?;
                out_bytes += entry.size;
            }
            entries_done += 1;
            ctx.report(ProgressEvent::Advanced {
                bytes_done: out_bytes,
                entries_done,
                current_path: path,
            });
        }

        Ok(JobStats {
            in_bytes: fs::metadata(archive).map(|m| m.len()).unwrap_or(0),
            out_bytes,
            duration: start.elapsed(),
        })
    }
}

/// Entry name normalized to forward slashes (Windows-produced rars use `\`).
fn entry_path(name: &str) -> PathBuf {
    PathBuf::from(name.replace('\\', "/"))
}

/// Map the dll.hpp codes: password problems are `PasswordRequired` (out of
/// scope v1, docs/03 D2), everything else on this decode-only path is a
/// corrupt archive. `ABORTED` without a stored write error means the shim
/// refused a volume change or oversize dictionary — also corrupt for v1.
fn map_rar(err: RarError) -> SquashError {
    if err.is_password() {
        SquashError::PasswordRequired
    } else {
        SquashError::CorruptArchive
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

    #[test]
    fn slash_path_keeps_rar_names_renderable() {
        assert_eq!(
            super::super::slash_path(&entry_path("dir\\тест.txt")),
            "dir/тест.txt"
        );
    }
}
