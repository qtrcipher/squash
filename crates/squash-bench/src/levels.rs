//! Honest preset → competitor level mapping (docs/01 §4 claims, docs/05 §6).
//!
//! Squash presets are per-format codec levels (`squash_core::presets`). A
//! fair comparison passes the **same numeric level** of the **same codec**
//! to the competitor's own level flag:
//!
//! | squash format | preset levels | competitor | competitor flag |
//! |---|---|---|---|
//! | zip | deflate 1 / 6 / 9 | `7zz a -tzip` | `-mx=1/6/9` |
//! | 7z | LZMA2 1 / 5 / 9 | `7zz a -t7z` | `-mx=1/5/9` (5 = 7-Zip default) |
//! | tar.gz | deflate 1 / 6 / 9 | `gzip` on a tar of the set | `-1/-6/-9` (6 = gzip default) |
//! | tar.zst | zstd 3 / 7 / 19 | `zstd` on a tar of the set | `-3/-7/-19` (3 = zstd default) |
//! | tar.xz | (reference only) | `xz` on a tar of the set | `-1/-6/-9` (6 = xz default) |
//!
//! So each `balanced` row is also the *competitor's own default* — the
//! docs/01 §4 "match/beat 7-Zip's default" claim is measured head-on.
//! Differences that remain (container overhead, threading, implementation)
//! are exactly what the product comparison is about. xz has no squash
//! tar.xz create path (extract-only, docs/05 §4); it runs as a reference.
//!
//! GUI-only tools (Keka) have no CLI surface and rar creation is
//! license-forbidden (WinRAR) — both are reported as skipped, never hidden.

use squash_core::format::Format;
use squash_core::presets::{self, Preset};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Competitor {
    SevenZip,
    Gzip,
    Zstd,
    Xz,
}

impl Competitor {
    pub const ALL: [Competitor; 4] = [Self::SevenZip, Self::Gzip, Self::Zstd, Self::Xz];

    /// Report/JSON label.
    pub fn label(self) -> &'static str {
        match self {
            Self::SevenZip => "7zz",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
            Self::Xz => "xz",
        }
    }

    /// Binary names tried on PATH, in preference order.
    pub fn binaries(self) -> &'static [&'static str] {
        match self {
            Self::SevenZip => &["7zz", "7z"],
            Self::Gzip => &["gzip"],
            Self::Zstd => &["zstd"],
            Self::Xz => &["xz"],
        }
    }

    /// Archive shapes this competitor benchmarks, each paired with the
    /// squash format whose preset table supplies the level. The label is
    /// what the report shows (a tar of the set + codec = the tar.* shape).
    pub fn compared_formats(self) -> &'static [(Format, &'static str)] {
        match self {
            Self::SevenZip => &[(Format::Zip, "zip"), (Format::SevenZ, "7z")],
            Self::Gzip => &[(Format::TarGz, "tar.gz")],
            Self::Zstd => &[(Format::TarZst, "tar.zst")],
            // Squash has no tar.xz create path: xz runs as a reference row
            // keyed to the Xz preset levels (1/6/9).
            Self::Xz => &[(Format::Xz, "tar.xz")],
        }
    }

    /// Codec CLIs write the compressed stream to stdout (`-c`); 7zz takes an
    /// output path argument instead.
    pub fn writes_to_stdout(self) -> bool {
        !matches!(self, Self::SevenZip)
    }

    /// Competitors that consume a prepared tar of the set (single-stream
    /// codec CLIs) vs. a directory (7zz archives the directory itself).
    pub fn needs_tar_input(self) -> bool {
        !matches!(self, Self::SevenZip)
    }
}

/// The level to hand the competitor for a squash (format, preset) pair:
/// the preset table's own level, unmodified. `None` for extract-only pairs.
pub fn competitor_level(format: Format, preset: Preset) -> Option<u8> {
    presets::params(format, preset).map(|p| p.level)
}

/// Compress arguments (no input/output — see below):
/// - 7zz: `[a, -t<fmt>, -mx<L>]` then caller appends `<output> <input>`.
/// - codecs: `[-c, -<L>]` then caller appends `<input>`; stdout → output file.
pub fn compress_args(c: Competitor, format: Format, level: u8) -> Vec<String> {
    match c {
        Competitor::SevenZip => {
            let t = match format {
                Format::Zip => "-tzip",
                Format::SevenZ => "-t7z",
                _ => unreachable!("7zz only compared on zip/7z"),
            };
            vec!["a".into(), t.into(), format!("-mx={level}")]
        }
        Competitor::Gzip => vec!["-c".into(), format!("-{level}")],
        Competitor::Zstd => vec!["-q".into(), "-c".into(), format!("-{level}")],
        Competitor::Xz => vec!["-c".into(), format!("-{level}")],
    }
}

/// Decompress arguments: 7zz extracts into a directory (`x -y -o<dir>`,
/// caller appends), codecs stream to stdout (`-dc`, caller appends archive).
pub fn decompress_args(c: Competitor) -> Vec<String> {
    match c {
        Competitor::SevenZip => vec!["x".into(), "-y".into()],
        Competitor::Gzip | Competitor::Xz => vec!["-dc".into()],
        Competitor::Zstd => vec!["-q".into(), "-dc".into()],
    }
}

/// Find the first available binary for `candidates` on PATH.
pub fn find_on_path(candidates: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in candidates {
            let full = dir.join(name);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

/// A competitor that is present and benchmarkable.
#[derive(Debug, Clone)]
pub struct Detected {
    pub tool: Competitor,
    pub binary: PathBuf,
}

/// Detect every competitor, returning the available ones plus a clear skip
/// line for everything not benchmarked — absent binaries, GUI-only tools,
/// and the license-blocked ones (docs/01 §4: comparisons must be honest
/// about what was *not* measured).
pub fn detect_all() -> (Vec<Detected>, Vec<String>) {
    let mut found = Vec::new();
    let mut skipped = Vec::new();
    for tool in Competitor::ALL {
        match find_on_path(tool.binaries()) {
            Some(binary) => found.push(Detected { tool, binary }),
            None => skipped.push(format!(
                "{}: skipped (not on PATH; install {} for the full comparison)",
                tool.label(),
                match tool {
                    Competitor::SevenZip => "p7zip",
                    other => other.label(),
                }
            )),
        }
    }
    skipped.push("keka: skipped (GUI-only — no CLI benchmark surface)".to_string());
    skipped.push(
        "winrar: skipped (rar creation is license-forbidden for Squash; rar CLI not benchmarked)"
            .to_string(),
    );
    (found, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn competitor_level_mirrors_preset_table() {
        // The honesty contract: same codec, same numeric level.
        assert_eq!(competitor_level(Format::Zip, Preset::Fast), Some(1));
        assert_eq!(competitor_level(Format::Zip, Preset::Balanced), Some(6));
        assert_eq!(competitor_level(Format::SevenZ, Preset::Balanced), Some(5));
        assert_eq!(competitor_level(Format::TarGz, Preset::Max), Some(9));
        assert_eq!(competitor_level(Format::TarZst, Preset::Fast), Some(3));
        assert_eq!(competitor_level(Format::TarZst, Preset::Max), Some(19));
        assert_eq!(competitor_level(Format::Xz, Preset::Balanced), Some(6));
        // Extract-only formats have no level to map.
        assert_eq!(competitor_level(Format::Tar, Preset::Balanced), None);
        assert_eq!(competitor_level(Format::Rar, Preset::Balanced), None);
    }

    #[test]
    fn balanced_rows_are_competitor_defaults() {
        // docs/01 §4 claims are about *defaults*: balanced must equal each
        // competitor's out-of-box level (7-Zip 5, gzip 6, zstd 3, xz 6).
        assert_eq!(competitor_level(Format::SevenZ, Preset::Balanced), Some(5));
        assert_eq!(competitor_level(Format::TarGz, Preset::Balanced), Some(6));
        assert_eq!(competitor_level(Format::TarZst, Preset::Fast), Some(3));
        assert_eq!(competitor_level(Format::Xz, Preset::Balanced), Some(6));
    }

    #[test]
    fn sevenz_compress_args() {
        assert_eq!(
            compress_args(Competitor::SevenZip, Format::Zip, 6),
            ["a", "-tzip", "-mx=6"]
        );
        assert_eq!(
            compress_args(Competitor::SevenZip, Format::SevenZ, 9),
            ["a", "-t7z", "-mx=9"]
        );
    }

    #[test]
    fn codec_compress_args_stream_to_stdout() {
        assert_eq!(
            compress_args(Competitor::Gzip, Format::TarGz, 1),
            ["-c", "-1"]
        );
        assert_eq!(
            compress_args(Competitor::Zstd, Format::TarZst, 19),
            ["-q", "-c", "-19"]
        );
        assert_eq!(compress_args(Competitor::Xz, Format::Xz, 9), ["-c", "-9"]);
        for c in [Competitor::Gzip, Competitor::Zstd, Competitor::Xz] {
            assert!(c.writes_to_stdout());
            assert!(c.needs_tar_input());
        }
        assert!(!Competitor::SevenZip.writes_to_stdout());
        assert!(!Competitor::SevenZip.needs_tar_input());
    }

    #[test]
    fn decompress_args_shape() {
        assert_eq!(decompress_args(Competitor::SevenZip), ["x", "-y"]);
        assert_eq!(decompress_args(Competitor::Gzip), ["-dc"]);
        assert_eq!(decompress_args(Competitor::Zstd), ["-q", "-dc"]);
        assert_eq!(decompress_args(Competitor::Xz), ["-dc"]);
    }

    #[test]
    fn skip_lines_cover_gui_and_license_cases() {
        let (_found, skipped) = detect_all();
        // Keka and WinRAR are never silently absent from the report.
        assert!(skipped.iter().any(|l| l.starts_with("keka:")));
        assert!(skipped.iter().any(|l| l.starts_with("winrar:")));
        // Every competitor is either detected or has a skip line.
        for tool in Competitor::ALL {
            let _found = _found.iter().find(|d| d.tool == tool);
            assert!(
                _found.is_some() || skipped.iter().any(|l| l.starts_with(tool.label())),
                "{} neither detected nor explained",
                tool.label()
            );
        }
    }
}
