//! Extract layout rule (docs/03 F3): an archive whose root is a single
//! folder extracts as-is into the destination; an archive with loose files
//! at its root goes into a **new folder named after the archive**
//! (`<archive-stem>/`) so extraction never explodes over the destination.

use crate::format::Format;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// One archive entry as seen by the layout classifier: path + directory flag.
pub struct EntryMeta {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Decide the real on-disk directory an archive should extract into.
///
/// - All entries share one first component that is a directory (the classic
///   `photos/…` archive) → extract as-is: returns `dest` unchanged.
/// - Anything else (loose files, several roots, a single loose file) →
///   returns `dest/<archive-stem>/`, creating the anti-desktop-explosion
///   folder named after the archive with its format extension stripped
///   (`backup.tar.gz` → `backup/`).
pub fn extraction_target(
    dest: &Path,
    archive: &Path,
    format: Format,
    entries: &[EntryMeta],
) -> PathBuf {
    if has_single_root_folder(entries) {
        log::debug!(
            "layout: {} has a single root folder — extracting as-is into {}",
            archive.display(),
            dest.display()
        );
        dest.to_path_buf()
    } else {
        let folder = dest.join(archive_stem(archive, format));
        log::debug!(
            "layout: {} has loose/multiple roots — extracting into new folder {}",
            archive.display(),
            folder.display()
        );
        folder
    }
}

/// True iff every entry lives under the same single root folder.
fn has_single_root_folder(entries: &[EntryMeta]) -> bool {
    let mut root: Option<&OsStr> = None;
    let mut root_is_folder = false;
    for entry in entries {
        let mut components = entry.path.components();
        let Some(Component::Normal(first)) = components.next() else {
            // Empty or "."-rooted entry: not a single clean root folder.
            return false;
        };
        match root {
            None => root = Some(first),
            Some(r) if r == first => {}
            _ => return false,
        }
        if components.next().is_some() || entry.is_dir {
            root_is_folder = true;
        }
    }
    root.is_some() && root_is_folder
}

/// Archive file name minus its format extension: `backup.tar.gz` → `backup`,
/// `x.tgz` → `x`. Falls back to the plain file stem for unknown suffixes.
pub fn archive_stem(archive: &Path, format: Format) -> String {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".to_string());
    let lower = name.to_ascii_lowercase();
    for ext in format.extensions() {
        let suffix = format!(".{ext}");
        if lower.ends_with(&suffix) {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    archive
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, is_dir: bool) -> EntryMeta {
        EntryMeta {
            path: PathBuf::from(path),
            is_dir,
        }
    }

    fn target(entries: &[EntryMeta]) -> PathBuf {
        extraction_target(
            Path::new("/dest"),
            Path::new("/archives/backup.tar.gz"),
            Format::TarGz,
            entries,
        )
    }

    #[test]
    fn single_root_folder_extracts_as_is() {
        let entries = [
            entry("photos", true),
            entry("photos/cat.jpg", false),
            entry("photos/nested/dog.jpg", false),
        ];
        assert_eq!(target(&entries), PathBuf::from("/dest"));
    }

    #[test]
    fn single_root_without_explicit_dir_entry_still_as_is() {
        // zip files often omit the explicit `photos/` directory entry.
        let entries = [
            entry("photos/cat.jpg", false),
            entry("photos/dog.jpg", false),
        ];
        assert_eq!(target(&entries), PathBuf::from("/dest"));
    }

    #[test]
    fn loose_files_go_into_archive_named_folder() {
        let entries = [entry("a.txt", false), entry("b.txt", false)];
        assert_eq!(target(&entries), PathBuf::from("/dest/backup"));
    }

    #[test]
    fn mixed_root_and_folder_goes_into_archive_named_folder() {
        let entries = [entry("readme.md", false), entry("photos/cat.jpg", false)];
        assert_eq!(target(&entries), PathBuf::from("/dest/backup"));
    }

    #[test]
    fn single_loose_file_goes_into_archive_named_folder() {
        let entries = [entry("readme.md", false)];
        assert_eq!(target(&entries), PathBuf::from("/dest/backup"));
    }

    #[test]
    fn two_root_folders_go_into_archive_named_folder() {
        let entries = [entry("a/x", false), entry("b/y", false)];
        assert_eq!(target(&entries), PathBuf::from("/dest/backup"));
    }

    #[test]
    fn empty_archive_goes_into_archive_named_folder() {
        assert_eq!(target(&[]), PathBuf::from("/dest/backup"));
    }

    #[test]
    fn stem_strips_full_format_extension() {
        assert_eq!(
            archive_stem(Path::new("/a/backup.tar.gz"), Format::TarGz),
            "backup"
        );
        assert_eq!(archive_stem(Path::new("x.TGZ"), Format::TarGz), "x");
        assert_eq!(archive_stem(Path::new("photo.zip"), Format::Zip), "photo");
        assert_eq!(
            archive_stem(Path::new("data.tar.zst"), Format::TarZst),
            "data"
        );
        // No known suffix → plain file stem.
        assert_eq!(archive_stem(Path::new("odd.bin"), Format::Zip), "odd");
    }

    #[test]
    fn stem_preserves_unicode() {
        assert_eq!(
            archive_stem(Path::new("نسخة احتياطية.zip"), Format::Zip),
            "نسخة احتياطية"
        );
    }
}
