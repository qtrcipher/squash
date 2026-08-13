//! Format handlers (docs/05 §4). One module per provider crate, all wired
//! into [`crate::FormatRegistry`] by [`register_builtin`].
//!
//! Implemented: zip and 7z (create+extract), tar.gz / tar.zst
//! (create+extract), tar / tar.bz2 / tar.xz (extract-only), gz / xz / zst
//! single-file codecs (create+extract), and rar (extract-only, behind the
//! default `rar` feature — the RARLAB license forbids RAR creation; see
//! vendor/unrar/README.squash.md).

#[cfg(feature = "rar")]
pub mod rar;
pub mod sevenz;
pub mod single_file;
pub mod tar_family;
pub mod zip;

use crate::error::SquashError;
use crate::format::{Format, FormatRegistry};
use crate::presets::{self, Preset};
use std::io;
use std::path::{Component, Path, PathBuf};

pub fn register_builtin(registry: &mut FormatRegistry) {
    registry.register(Box::new(zip::ZipHandler));
    registry.register(Box::new(sevenz::SevenZHandler));
    #[cfg(feature = "rar")]
    registry.register(Box::new(rar::RarHandler));
    registry.register(Box::new(tar_family::TarHandler));
    registry.register(Box::new(tar_family::TarGzHandler));
    registry.register(Box::new(tar_family::TarBz2Handler));
    registry.register(Box::new(tar_family::TarXzHandler));
    registry.register(Box::new(tar_family::TarZstHandler));
    registry.register(Box::new(single_file::GzHandler));
    registry.register(Box::new(single_file::XzHandler));
    registry.register(Box::new(single_file::ZstHandler));
}

/// Look up a preset's level from [`crate::presets::PRESET_TABLE`] and clamp
/// it to the docs/06 §2 bounds for the format.
pub(crate) fn clamped_level(format: Format, preset: Preset) -> Result<u8, SquashError> {
    let params = presets::params(format, preset).ok_or(SquashError::UnsupportedFormat)?;
    let (lo, hi) = presets::level_bounds(format).ok_or(SquashError::UnsupportedFormat)?;
    Ok(params.level.clamp(lo, hi))
}

/// Map a filesystem error to the taxonomy. `ENOSPC` (unix 28, Windows 112)
/// is [`SquashError::DiskFull`]; everything unexpected is `Internal` — the
/// taxonomy is deliberately small (docs/05 §3).
pub(crate) fn map_io(err: io::Error) -> SquashError {
    match err.kind() {
        io::ErrorKind::PermissionDenied => SquashError::PermissionDenied,
        io::ErrorKind::StorageFull => SquashError::DiskFull,
        _ => match err.raw_os_error() {
            Some(28) | Some(112) => SquashError::DiskFull,
            _ => SquashError::Internal,
        },
    }
}

/// Map an error from a *decode-only* position (reading/decompressing archive
/// bytes: header parse, entries iteration, payload reads). Malformed data
/// surfaces as `InvalidData`/`InvalidInput`/`UnexpectedEof`, and some
/// decoders (zstd) report every failure as `Other` — in a decode-only
/// position `Other` can only come from the decoder, so it maps to
/// [`SquashError::CorruptArchive`] too. Real I/O kinds (permissions, disk
/// full) still route through [`map_io`].
pub(crate) fn map_decode(err: io::Error) -> SquashError {
    match err.kind() {
        io::ErrorKind::InvalidData
        | io::ErrorKind::InvalidInput
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::Other => SquashError::CorruptArchive,
        _ => map_io(err),
    }
}

/// Combined mapping for `io::copy`-style operations where read (decode) and
/// write (disk) errors share one channel: decode-shaped kinds mean corrupt
/// input, the rest go through [`map_io`]. Prefer split read/write loops
/// (see `tar_family::copy_entry`) when the decoder reports `Other`.
pub(crate) fn map_transfer(err: io::Error) -> SquashError {
    match err.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput | io::ErrorKind::UnexpectedEof => {
            SquashError::CorruptArchive
        }
        _ => map_io(err),
    }
}

/// One file/dir/symlink discovered while walking the compress inputs.
#[derive(Debug)]
pub(crate) struct WalkEntry {
    /// On-disk path.
    pub fs_path: PathBuf,
    /// Path inside the archive (`<input-name>/<relative>`).
    pub archive_path: PathBuf,
    pub is_dir: bool,
    /// Uncompressed size (0 for dirs/symlinks).
    pub len: u64,
}

/// Walk the compress inputs depth-first, assigning each entry its archive
/// path: each input keeps its own file name as the top-level component
/// (`squash c src/ docs/` → `src/…`, `docs/…`).
pub(crate) fn walk_inputs(inputs: &[PathBuf]) -> Result<Vec<WalkEntry>, SquashError> {
    let mut out = Vec::new();
    for input in inputs {
        let base = input.file_name().ok_or(SquashError::Internal)?;
        for item in walkdir::WalkDir::new(input).follow_links(false) {
            let item = item.map_err(|e| {
                e.into_io_error()
                    .map(map_io)
                    .unwrap_or(SquashError::Internal)
            })?;
            let fs_path = item.path().to_path_buf();
            let archive_path = if fs_path == *input {
                PathBuf::from(base)
            } else {
                let rel = fs_path
                    .strip_prefix(input)
                    .map_err(|_| SquashError::Internal)?;
                let mut p = PathBuf::from(base);
                p.push(rel);
                p
            };
            out.push(WalkEntry {
                len: if item.file_type().is_file() {
                    item.metadata().map(|m| m.len()).unwrap_or(0)
                } else {
                    0
                },
                is_dir: item.file_type().is_dir(),
                fs_path,
                archive_path,
            });
        }
    }
    Ok(out)
}

/// Total input bytes for the compress `Started` estimate.
pub fn inputs_total_bytes(inputs: &[PathBuf]) -> Option<u64> {
    walk_inputs(inputs)
        .ok()
        .map(|entries| entries.iter().map(|e| e.len).sum())
}

/// Render an archive path with forward slashes (zip requires `/` separators
/// on every OS).
pub(crate) fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_path_uses_forward_slashes() {
        assert_eq!(slash_path(Path::new("a/b/c.txt")), "a/b/c.txt");
        assert_eq!(slash_path(Path::new("ملف.txt")), "ملف.txt");
    }

    #[test]
    fn clamped_level_comes_from_preset_table() {
        // docs/05 §3 anchor: fast tar.zst = zstd 3.
        assert_eq!(clamped_level(Format::TarZst, Preset::Fast).unwrap(), 3);
        assert_eq!(clamped_level(Format::Zip, Preset::Max).unwrap(), 9);
        // Single-file codecs share the same table/bounds machinery.
        assert_eq!(clamped_level(Format::Gz, Preset::Balanced).unwrap(), 6);
        assert_eq!(clamped_level(Format::Zst, Preset::Max).unwrap(), 19);
        // Extract-only formats have no preset row.
        assert_eq!(
            clamped_level(Format::Tar, Preset::Balanced),
            Err(SquashError::UnsupportedFormat)
        );
    }

    #[test]
    fn walk_inputs_prefixes_each_input_name() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested/f.txt"), b"hello").unwrap();
        let entries = walk_inputs(std::slice::from_ref(&dir)).unwrap();
        let names: Vec<String> = entries
            .iter()
            .map(|e| slash_path(&e.archive_path))
            .collect();
        assert_eq!(names, ["src", "src/nested", "src/nested/f.txt"]);
        let file = entries.iter().find(|e| !e.is_dir).unwrap();
        assert_eq!(file.len, 5);
    }

    #[test]
    fn walk_inputs_missing_path_is_error() {
        let err = walk_inputs(&[PathBuf::from("/definitely/not/here")]).unwrap_err();
        assert!(matches!(
            err,
            SquashError::Internal | SquashError::PermissionDenied
        ));
    }
}
