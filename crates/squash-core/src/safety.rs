//! Extraction path sanitizer — the zip-slip safety layer (docs/05 §3).
//!
//! Every entry path of every format passes through here before a single byte
//! is written; handlers cannot bypass it. Posture (docs/03 F7): a blocked
//! entry aborts the whole job with [`SquashError::PathTraversalBlocked`] —
//! no override switch, no silent skipping.
//!
//! Two checks compose to keep writes inside the destination:
//! - [`sanitize_entry_path`] rejects absolute paths, Windows prefixes and any
//!   `..` component in an entry's own path.
//! - [`sanitize_link_target`] lexically resolves a symlink/hardlink target and
//!   verifies it stays inside the destination, so a later entry can never be
//!   written *through* a planted link to the outside.

use crate::error::SquashError;
use std::path::{Component, Path, PathBuf};

/// Map an archive entry path to a safe on-disk path under `dest`.
///
/// Rejects (returns [`SquashError::PathTraversalBlocked`]):
/// - absolute paths (`/etc/passwd`, `C:\…`, UNC prefixes),
/// - any `..` component — even ones that would lexically stay inside
///   (`a/../b`); strictness here keeps the rule auditable,
/// - empty entry names.
pub fn sanitize_entry_path(dest: &Path, entry: &Path) -> Result<PathBuf, SquashError> {
    let mut out = dest.to_path_buf();
    let mut components = 0usize;
    for component in entry.components() {
        match component {
            Component::Normal(part) => {
                out.push(part);
                components += 1;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                log::debug!(
                    "sanitizer blocked entry path {:?} (dest {})",
                    entry,
                    dest.display()
                );
                return Err(SquashError::PathTraversalBlocked);
            }
        }
    }
    if components == 0 {
        log::debug!(
            "sanitizer blocked empty entry path {:?} (dest {})",
            entry,
            dest.display()
        );
        return Err(SquashError::PathTraversalBlocked);
    }
    Ok(out)
}

/// Validate a link (`symlink`/`hardlink`) target.
///
/// `link_path` is the already-sanitized on-disk path of the link itself,
/// `target` the raw target stored in the archive. The target is resolved
/// lexically relative to the link's parent and must stay inside `dest`;
/// absolute targets are rejected outright. Returns the resolved on-disk path
/// the link would point at.
pub fn sanitize_link_target(
    dest: &Path,
    link_path: &Path,
    target: &Path,
) -> Result<PathBuf, SquashError> {
    debug_assert!(link_path.starts_with(dest));
    if target.is_absolute() {
        log::debug!(
            "sanitizer blocked absolute link target {:?} (link {})",
            target,
            link_path.display()
        );
        return Err(SquashError::PathTraversalBlocked);
    }
    let mut resolved = link_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dest.to_path_buf());
    for component in target.components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                // Popping at `dest` itself would escape the destination.
                resolved.pop();
                if !resolved.starts_with(dest) {
                    log::debug!(
                        "sanitizer blocked escaping link target {:?} (link {})",
                        target,
                        link_path.display()
                    );
                    return Err(SquashError::PathTraversalBlocked);
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                log::debug!(
                    "sanitizer blocked link target {:?} (link {})",
                    target,
                    link_path.display()
                );
                return Err(SquashError::PathTraversalBlocked);
            }
        }
    }
    if !resolved.starts_with(dest) {
        log::debug!(
            "sanitizer blocked link target {:?} resolving outside {}",
            target,
            dest.display()
        );
        return Err(SquashError::PathTraversalBlocked);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dest() -> PathBuf {
        PathBuf::from("/tmp/squash-dest")
    }

    #[test]
    fn normal_relative_paths_pass() {
        assert_eq!(
            sanitize_entry_path(&dest(), Path::new("photos/cat.jpg")).unwrap(),
            dest().join("photos/cat.jpg")
        );
        assert_eq!(
            sanitize_entry_path(&dest(), Path::new("file.txt")).unwrap(),
            dest().join("file.txt")
        );
    }

    #[test]
    fn dot_components_are_inert() {
        assert_eq!(
            sanitize_entry_path(&dest(), Path::new("./a/./b")).unwrap(),
            dest().join("a/b")
        );
    }

    #[test]
    fn parent_traversal_is_blocked() {
        for evil in ["../evil", "a/../../evil", "..", "a/.."] {
            assert_eq!(
                sanitize_entry_path(&dest(), Path::new(evil)),
                Err(SquashError::PathTraversalBlocked),
                "{evil} must be blocked"
            );
        }
    }

    #[test]
    fn absolute_paths_are_blocked() {
        assert_eq!(
            sanitize_entry_path(&dest(), Path::new("/etc/passwd")),
            Err(SquashError::PathTraversalBlocked)
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_prefixes_are_blocked() {
        for evil in [r"C:\windows\system32", r"\\server\share\x"] {
            assert_eq!(
                sanitize_entry_path(&dest(), Path::new(evil)),
                Err(SquashError::PathTraversalBlocked)
            );
        }
    }

    #[test]
    fn empty_entry_is_blocked() {
        assert_eq!(
            sanitize_entry_path(&dest(), Path::new("")),
            Err(SquashError::PathTraversalBlocked)
        );
        assert_eq!(
            sanitize_entry_path(&dest(), Path::new(".")),
            Err(SquashError::PathTraversalBlocked)
        );
    }

    #[test]
    fn unicode_names_pass_through() {
        let name = "مجلد/ملف عربي.txt";
        assert_eq!(
            sanitize_entry_path(&dest(), Path::new(name)).unwrap(),
            dest().join(name)
        );
    }

    #[test]
    fn link_target_inside_dest_passes() {
        let link = dest().join("dir/link");
        assert_eq!(
            sanitize_link_target(&dest(), &link, Path::new("real.txt")).unwrap(),
            dest().join("dir/real.txt")
        );
        assert_eq!(
            sanitize_link_target(&dest(), &link, Path::new("../other.txt")).unwrap(),
            dest().join("other.txt")
        );
    }

    #[test]
    fn link_target_escaping_dest_is_blocked() {
        let link = dest().join("dir/link");
        for evil in ["../../outside", "../../../etc/passwd", "../.."] {
            assert_eq!(
                sanitize_link_target(&dest(), &link, Path::new(evil)),
                Err(SquashError::PathTraversalBlocked),
                "{evil} must be blocked"
            );
        }
    }

    #[test]
    fn absolute_link_target_is_blocked() {
        let link = dest().join("link");
        assert_eq!(
            sanitize_link_target(&dest(), &link, Path::new("/etc/passwd")),
            Err(SquashError::PathTraversalBlocked)
        );
    }

    #[test]
    fn link_at_dest_root_cannot_escape_via_parent() {
        let link = dest().join("link");
        assert_eq!(
            sanitize_link_target(&dest(), &link, Path::new("../outside")),
            Err(SquashError::PathTraversalBlocked)
        );
    }
}
