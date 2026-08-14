//! Extraction path sanitizer — the zip-slip safety layer (docs/05 §3).
//!
//! Every entry path of every format passes through here before a single byte
//! is written; handlers cannot bypass it. Posture (docs/03 F7): a blocked
//! entry aborts the whole job with [`SquashError::PathTraversalBlocked`] —
//! no override switch, no silent skipping.
//!
//! Checks that compose to keep writes inside the destination:
//! - [`sanitize_entry_path`] rejects absolute paths, Windows prefixes and any
//!   `..` component in an entry's own path.
//! - [`sanitize_link_target`] resolves a symlink/hardlink target **physically**
//!   (canonicalizing every prefix that exists on disk, so a `..` can never pop
//!   through a symlink) and verifies it stays inside the destination.
//! - [`create_dir_all_guarded`] / [`create_file_guarded`] create directories
//!   and files only after verifying no *existing* ancestor below the
//!   destination is a symlink — so a planted link can never be written
//!   through, closing the check-vs-write gap between entries.
//! - [`ExtractGuard`] is the decompression-bomb guard (docs/07 §2): ratio,
//!   absolute-size and entry-count caps on actual bytes written, plus
//!   rollback of everything the job created when it trips.

use crate::error::SquashError;
use crate::formats::map_io;
use std::fs::{self, File};
use std::io;
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
/// relative to the link's parent and must stay inside `dest`; absolute
/// targets are rejected outright. Returns the resolved on-disk path the link
/// would point at.
///
/// Resolution is **physical**, not lexical: every prefix of the path that
/// exists on disk is canonicalized (symlinks followed, `..` resolved by the
/// OS), so the classic chain attack — plant `c → .`, then `a → c/..`, where a
/// lexical check pops `c` but the filesystem pops `c`'s *target* and lands
/// outside `dest` — is rejected. Only non-existent tail components are
/// resolved lexically, which is exact because a component that does not exist
/// cannot be a symlink.
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
    // `dest` exists by the time links are validated (handlers create it in
    // their layout step). Canonicalizing it once anchors every comparison in
    // the physical tree — `starts_with` against a lexical `dest` would be
    // fooled by a `dest` reached through a symlink (e.g. /tmp on macOS).
    let canon_dest = fs::canonicalize(dest).map_err(map_io)?;
    let parent = link_path.parent().unwrap_or(dest);
    let mut resolved = resolve_existing_prefix(parent)?;
    if !resolved.starts_with(&canon_dest) {
        log::debug!(
            "sanitizer blocked link {} whose parent resolves outside {}",
            link_path.display(),
            dest.display()
        );
        return Err(SquashError::PathTraversalBlocked);
    }
    for component in target.components() {
        match component {
            Component::Normal(part) => {
                resolved.push(part);
                // The push may have entered an existing symlink — re-anchor.
                resolved = reanchor(&resolved, &canon_dest)?;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                // Popping lexically is only exact if the final component is a
                // real directory: canonicalize first so a symlink final
                // component is popped at its *target*, like the OS does.
                resolved = reanchor(&resolved, &canon_dest)?;
                resolved.pop();
                if !resolved.starts_with(&canon_dest) {
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
    if !resolved.starts_with(&canon_dest) {
        log::debug!(
            "sanitizer blocked link target {:?} resolving outside {}",
            target,
            dest.display()
        );
        return Err(SquashError::PathTraversalBlocked);
    }
    Ok(resolved)
}

/// Canonicalize the longest existing prefix of `path` and append the
/// remaining (non-existent) components lexically. The result equals the
/// physical path `path` would have if created right now: existing prefixes
/// are symlink-resolved by the OS; non-existent tails cannot contain
/// symlinks by definition.
fn resolve_existing_prefix(path: &Path) -> Result<PathBuf, SquashError> {
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    let mut cursor = path;
    while !cursor.exists() {
        match cursor.file_name() {
            Some(name) => tail.push(name),
            None => return Err(SquashError::Internal), // hit a root: give up
        }
        cursor = cursor.parent().ok_or(SquashError::Internal)?;
    }
    let mut resolved = fs::canonicalize(cursor).map_err(map_io)?;
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

/// If `path` exists, replace it with its canonical (physical) form and
/// verify it is still inside `canon_dest`; if it does not exist, its current
/// form is already exact (see [`resolve_existing_prefix`]) — pass through.
fn reanchor(path: &Path, canon_dest: &Path) -> Result<PathBuf, SquashError> {
    if path.exists() {
        let canon = fs::canonicalize(path).map_err(map_io)?;
        if !canon.starts_with(canon_dest) {
            return Err(SquashError::PathTraversalBlocked);
        }
        Ok(canon)
    } else {
        Ok(path.to_path_buf())
    }
}

/// Create every missing directory of `path` (a directory path already
/// sanitized under `dest`), refusing to descend *through* an existing
/// symlink: without this check `create_dir_all` would happily create
/// directories on the far side of a planted link. Directories created here
/// are tracked in `guard` for rollback.
pub fn create_dir_all_guarded(
    dest: &Path,
    path: &Path,
    guard: &mut ExtractGuard,
) -> Result<(), SquashError> {
    let rel = path.strip_prefix(dest).map_err(|_| SquashError::Internal)?;
    let mut cur = dest.to_path_buf();
    for component in rel.components() {
        // `rel` comes from sanitize_entry_path: only Normal components.
        let Component::Normal(part) = component else {
            return Err(SquashError::Internal);
        };
        cur.push(part);
        match fs::symlink_metadata(&cur) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    log::debug!("sanitizer blocked write through symlink {}", cur.display());
                    return Err(SquashError::PathTraversalBlocked);
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&cur).map_err(map_io)?;
                guard.track_created(cur.clone());
            }
            Err(err) => return Err(map_io(err)),
        }
    }
    Ok(())
}

/// Create `path` (a file path already sanitized under `dest`) for writing,
/// after guarding the parent chain ([`create_dir_all_guarded`]) and refusing
/// to overwrite an existing **symlink** (a pre-existing link could point
/// anywhere — truncating it would write outside `dest`). Newly created files
/// are tracked in `guard` for rollback.
pub fn create_file_guarded(
    dest: &Path,
    path: &Path,
    guard: &mut ExtractGuard,
) -> Result<File, SquashError> {
    if let Some(parent) = path.parent() {
        create_dir_all_guarded(dest, parent, guard)?;
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            log::debug!("sanitizer blocked overwrite of symlink {}", path.display());
            return Err(SquashError::PathTraversalBlocked);
        }
    }
    let fresh = !path.exists();
    let file = File::create(path).map_err(map_io)?;
    if fresh {
        guard.track_created(path.to_path_buf());
    }
    Ok(file)
}

/// Refuse `path` (already sanitized under `dest`) if any *existing* ancestor
/// below `dest` — or `path` itself — is a symlink. Used for hardlink
/// sources, whose platform behavior around symlinks varies.
pub fn reject_symlink_components(dest: &Path, path: &Path) -> Result<(), SquashError> {
    let rel = path.strip_prefix(dest).map_err(|_| SquashError::Internal)?;
    let mut cur = dest.to_path_buf();
    for component in rel.components() {
        let Component::Normal(part) = component else {
            return Err(SquashError::Internal);
        };
        cur.push(part);
        if let Ok(meta) = fs::symlink_metadata(&cur) {
            if meta.file_type().is_symlink() {
                log::debug!("sanitizer blocked symlink component {}", cur.display());
                return Err(SquashError::PathTraversalBlocked);
            }
        }
    }
    Ok(())
}

// --- decompression-bomb guard (docs/07 §2) -----------------------------------

/// Limits for [`ExtractGuard`]. Defaults are the product contract
/// (docs/07 §2); the `SQUASH_EXTRACT_*` env vars override them for power
/// users and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractLimits {
    /// Abort when expanded bytes exceed `max_ratio × compressed_bytes`…
    pub max_ratio: u64,
    /// …but only once expanded bytes pass this floor, so small archives with
    /// legitimately high ratios (tiny files compress extremely well) never
    /// trip the guard.
    pub ratio_floor: u64,
    /// Absolute cap on total expanded bytes per job.
    pub max_total_bytes: u64,
    /// Cap on archive entries per job (entry-table floods).
    pub max_entries: u64,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_ratio: 200,
            ratio_floor: 64 * 1024 * 1024,              // 64 MiB
            max_total_bytes: 1024 * 1024 * 1024 * 1024, // 1 TiB
            max_entries: 1_000_000,
        }
    }
}

impl ExtractLimits {
    /// Defaults with `SQUASH_EXTRACT_MAX_RATIO`, `SQUASH_EXTRACT_MAX_BYTES`
    /// and `SQUASH_EXTRACT_MAX_ENTRIES` overrides (unparseable values are
    /// ignored — limits fail safe, never off).
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let read = |key: &str, fallback: u64| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(fallback)
        };
        Self {
            max_ratio: read("SQUASH_EXTRACT_MAX_RATIO", defaults.max_ratio),
            ratio_floor: defaults.ratio_floor,
            max_total_bytes: read("SQUASH_EXTRACT_MAX_BYTES", defaults.max_total_bytes),
            max_entries: read("SQUASH_EXTRACT_MAX_ENTRIES", defaults.max_entries),
        }
    }
}

/// Per-extraction-job bomb guard. Counts **actual bytes written** (never the
/// archive's declared sizes — those are attacker-controlled) against the
/// compressed input size, and remembers every path the job created so a
/// tripped guard can [`ExtractGuard::rollback`] the partial output.
pub struct ExtractGuard {
    limits: ExtractLimits,
    in_bytes: u64,
    out_bytes: u64,
    entries: u64,
    created: Vec<PathBuf>,
}

impl ExtractGuard {
    /// Guard with limits from [`ExtractLimits::from_env`]; `in_bytes` is the
    /// compressed archive's size on disk.
    pub fn new(in_bytes: u64) -> Self {
        Self::with_limits(in_bytes, ExtractLimits::from_env())
    }

    pub fn with_limits(in_bytes: u64, limits: ExtractLimits) -> Self {
        Self {
            limits,
            in_bytes,
            out_bytes: 0,
            entries: 0,
            created: Vec::new(),
        }
    }

    /// Count one more archive entry.
    pub fn record_entry(&mut self) -> Result<(), SquashError> {
        self.entries += 1;
        if self.entries > self.limits.max_entries {
            log::debug!("bomb guard: entry count {} exceeds cap", self.entries);
            return Err(SquashError::DecompressionBomb);
        }
        Ok(())
    }

    /// Count `n` more expanded bytes (call per written chunk, not per entry,
    /// so a single huge entry cannot overshoot the caps by gigabytes).
    pub fn record_bytes(&mut self, n: u64) -> Result<(), SquashError> {
        self.out_bytes = self
            .out_bytes
            .checked_add(n)
            .ok_or(SquashError::DecompressionBomb)?;
        if self.out_bytes > self.limits.max_total_bytes {
            log::debug!("bomb guard: {} bytes exceeds absolute cap", self.out_bytes);
            return Err(SquashError::DecompressionBomb);
        }
        if self.out_bytes > self.limits.ratio_floor
            && self.out_bytes > self.limits.max_ratio.saturating_mul(self.in_bytes.max(1))
        {
            log::debug!(
                "bomb guard: ratio trip at {} out / {} in bytes",
                self.out_bytes,
                self.in_bytes
            );
            return Err(SquashError::DecompressionBomb);
        }
        Ok(())
    }

    /// Actual expanded bytes written so far (drives `JobStats::out_bytes`).
    pub fn out_bytes(&self) -> u64 {
        self.out_bytes
    }

    /// Remember a path this job created, for [`ExtractGuard::rollback`].
    pub fn track_created(&mut self, path: PathBuf) {
        self.created.push(path);
    }

    /// Best-effort removal of everything tracked, deepest first so
    /// directories are emptied before they are removed. Only paths the job
    /// itself created are tracked, so pre-existing user files are never
    /// touched; leftovers (a directory the user wrote into mid-job, say) are
    /// simply left behind.
    pub fn rollback(&self) {
        for path in self.created.iter().rev() {
            let Ok(meta) = fs::symlink_metadata(path) else {
                continue;
            };
            let result = if meta.is_dir() && !meta.file_type().is_symlink() {
                fs::remove_dir(path)
            } else {
                fs::remove_file(path)
            };
            if result.is_err() {
                log::debug!("rollback: could not remove {}", path.display());
            }
        }
    }
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
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(dest.join("dir")).unwrap();
        let link = dest.join("dir/link");
        let canon = fs::canonicalize(&dest).unwrap();
        assert_eq!(
            sanitize_link_target(&dest, &link, Path::new("real.txt")).unwrap(),
            canon.join("dir/real.txt")
        );
        assert_eq!(
            sanitize_link_target(&dest, &link, Path::new("../other.txt")).unwrap(),
            canon.join("other.txt")
        );
    }

    #[test]
    fn link_target_escaping_dest_is_blocked() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(dest.join("dir")).unwrap();
        let link = dest.join("dir/link");
        for evil in ["../../outside", "../../../etc/passwd", "../.."] {
            assert_eq!(
                sanitize_link_target(&dest, &link, Path::new(evil)),
                Err(SquashError::PathTraversalBlocked),
                "{evil} must be blocked"
            );
        }
    }

    #[test]
    fn absolute_link_target_is_blocked() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        let link = dest.join("link");
        assert_eq!(
            sanitize_link_target(&dest, &link, Path::new("/etc/passwd")),
            Err(SquashError::PathTraversalBlocked)
        );
    }

    #[test]
    fn link_at_dest_root_cannot_escape_via_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        let link = dest.join("link");
        assert_eq!(
            sanitize_link_target(&dest, &link, Path::new("../outside")),
            Err(SquashError::PathTraversalBlocked)
        );
    }

    #[cfg(unix)]
    #[test]
    fn link_chain_popping_through_symlink_is_blocked() {
        // The lexical-validation bypass: `c → .` passes any check, then
        // `a → c/..` pops `c` lexically (stays at dest) but the OS resolves
        // `c` first and pops its *target* — landing outside dest.
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        std::os::unix::fs::symlink(".", dest.join("c")).unwrap();
        assert_eq!(
            sanitize_link_target(&dest, &dest.join("a"), Path::new("c/..")),
            Err(SquashError::PathTraversalBlocked),
            "a → c/.. with c → . must be blocked (physical pop)"
        );
    }

    #[cfg(unix)]
    #[test]
    fn link_through_symlink_to_inside_dest_still_passes() {
        // Legit chains keep working: `sub` is real, `alias → sub`, and
        // `link → alias/file.txt` resolves inside dest.
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(dest.join("sub")).unwrap();
        std::os::unix::fs::symlink("sub", dest.join("alias")).unwrap();
        let canon = fs::canonicalize(&dest).unwrap();
        assert_eq!(
            sanitize_link_target(&dest, &dest.join("link"), Path::new("alias/file.txt")).unwrap(),
            canon.join("sub/file.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn link_parent_resolving_outside_dest_is_blocked() {
        // A pre-existing symlink inside dest pointing outside must anchor the
        // parent resolution check.
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        std::os::unix::fs::symlink(tmp.path(), dest.join("esc")).unwrap();
        assert_eq!(
            sanitize_link_target(&dest, &dest.join("esc/link"), Path::new("x")),
            Err(SquashError::PathTraversalBlocked)
        );
    }

    #[cfg(unix)]
    #[test]
    fn guarded_dir_creation_refuses_symlink_ancestors() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        std::os::unix::fs::symlink(tmp.path(), dest.join("planted")).unwrap();
        let mut guard = ExtractGuard::new(100);
        assert_eq!(
            create_dir_all_guarded(&dest, &dest.join("planted/newdir"), &mut guard),
            Err(SquashError::PathTraversalBlocked),
            "create_dir_all would have written outside dest through the link"
        );
        assert!(!tmp.path().join("newdir").exists());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_file_creation_refuses_to_clobber_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("victim"), dest.join("f")).unwrap();
        fs::write(tmp.path().join("victim"), b"precious").unwrap();
        let mut guard = ExtractGuard::new(100);
        assert_eq!(
            create_file_guarded(&dest, &dest.join("f"), &mut guard).map(|_| ()),
            Err(SquashError::PathTraversalBlocked)
        );
        assert_eq!(fs::read(tmp.path().join("victim")).unwrap(), b"precious");
    }

    #[test]
    fn guarded_creation_tracks_and_rollback_removes() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        let mut guard = ExtractGuard::new(100);
        create_dir_all_guarded(&dest, &dest.join("a/b"), &mut guard).unwrap();
        create_file_guarded(&dest, &dest.join("a/b/f.txt"), &mut guard).unwrap();
        assert!(dest.join("a/b/f.txt").exists());
        guard.rollback();
        assert!(
            !dest.join("a").exists(),
            "deepest-first rollback removes all"
        );
        assert!(dest.exists(), "dest itself is never tracked");
    }

    #[test]
    fn guard_ratio_floor_lets_small_archives_pass() {
        let mut guard = ExtractGuard::with_limits(
            10,
            ExtractLimits {
                ..ExtractLimits::default()
            },
        );
        // 1000:1 ratio but far below the 64 MiB floor: fine.
        guard.record_bytes(10_000).unwrap();
    }

    #[test]
    fn guard_ratio_trip() {
        let mut guard = ExtractGuard::with_limits(
            1024,
            ExtractLimits {
                max_ratio: 100,
                ratio_floor: 1024,
                max_total_bytes: u64::MAX,
                max_entries: u64::MAX,
            },
        );
        guard.record_bytes(100 * 1024).unwrap(); // 100:1 — at the line
        assert_eq!(
            guard.record_bytes(1024),
            Err(SquashError::DecompressionBomb),
            "101:1 past the floor trips"
        );
    }

    #[test]
    fn guard_absolute_cap_trip() {
        let mut guard = ExtractGuard::with_limits(
            u64::MAX, // ratio can never trip
            ExtractLimits {
                max_ratio: u64::MAX,
                ratio_floor: u64::MAX,
                max_total_bytes: 1000,
                max_entries: u64::MAX,
            },
        );
        guard.record_bytes(1000).unwrap();
        assert_eq!(guard.record_bytes(1), Err(SquashError::DecompressionBomb));
    }

    #[test]
    fn guard_entry_cap_trip() {
        let mut guard = ExtractGuard::with_limits(
            100,
            ExtractLimits {
                max_entries: 3,
                ..ExtractLimits::default()
            },
        );
        guard.record_entry().unwrap();
        guard.record_entry().unwrap();
        guard.record_entry().unwrap();
        assert_eq!(guard.record_entry(), Err(SquashError::DecompressionBomb));
    }

    #[test]
    fn guard_byte_count_never_overflows() {
        let mut guard = ExtractGuard::with_limits(100, ExtractLimits::default());
        guard.record_bytes(u64::MAX - 10).unwrap_err(); // absolute cap trips first
        let mut guard = ExtractGuard::with_limits(
            100,
            ExtractLimits {
                max_ratio: u64::MAX,
                ratio_floor: u64::MAX,
                max_total_bytes: u64::MAX,
                max_entries: u64::MAX,
            },
        );
        guard.record_bytes(u64::MAX - 10).unwrap();
        assert_eq!(guard.record_bytes(20), Err(SquashError::DecompressionBomb));
    }
}
