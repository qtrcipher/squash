//! Format registry (docs/05 §3–§4).
//!
//! Detection is magic-bytes-first, extension as hint (P3 scripters feed it
//! pipes). Extension matching is implemented; magic-byte sniffing is a
//! documented follow-up (handlers receive an explicit `Format` today).
//!
//! Adding a format = one module implementing [`FormatHandler`] + registry
//! entry + fixtures. No changes to the job model, presets, CLI, or GUI.

use crate::error::SquashError;
use crate::job::Job;
use crate::progress::{JobStats, ProgressEvent};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Archive/codec formats, per the strategy table in docs/05 §4.
/// Serialized names match docs/06 (`zip`, `7z`, `tar.gz`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
    /// Extract-only, forever (RarLAB license forbids RAR creation).
    #[serde(rename = "rar")]
    Rar,
    #[serde(rename = "tar")]
    Tar,
    #[serde(rename = "tar.gz")]
    TarGz,
    /// Extract-only (matches the MVP compress list).
    #[serde(rename = "tar.bz2")]
    TarBz2,
    /// Extract-only.
    #[serde(rename = "tar.xz")]
    TarXz,
    #[serde(rename = "tar.zst")]
    TarZst,
    #[serde(rename = "gz")]
    Gz,
    #[serde(rename = "xz")]
    Xz,
    #[serde(rename = "zst")]
    Zst,
}

impl Format {
    pub const ALL: [Format; 11] = [
        Self::Zip,
        Self::SevenZ,
        Self::Rar,
        Self::Tar,
        Self::TarGz,
        Self::TarBz2,
        Self::TarXz,
        Self::TarZst,
        Self::Gz,
        Self::Xz,
        Self::Zst,
    ];

    /// Formats Squash can create (docs/01 §3.2, docs/06 `default_format`).
    pub const CREATE_CAPABLE: [Format; 4] = [Self::Zip, Self::SevenZ, Self::TarGz, Self::TarZst];

    /// Canonical name, matching the serde representation.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::Rar => "rar",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::TarBz2 => "tar.bz2",
            Self::TarXz => "tar.xz",
            Self::TarZst => "tar.zst",
            Self::Gz => "gz",
            Self::Xz => "xz",
            Self::Zst => "zst",
        }
    }

    /// Recognized extensions (without dot), most specific first.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Zip => &["zip"],
            Self::SevenZ => &["7z"],
            Self::Rar => &["rar"],
            Self::Tar => &["tar"],
            Self::TarGz => &["tar.gz", "tgz"],
            Self::TarBz2 => &["tar.bz2", "tbz2"],
            Self::TarXz => &["tar.xz", "txz"],
            Self::TarZst => &["tar.zst", "tzst"],
            Self::Gz => &["gz"],
            Self::Xz => &["xz"],
            Self::Zst => &["zst"],
        }
    }

    pub fn can_create(&self) -> bool {
        Self::CREATE_CAPABLE.contains(self)
    }

    /// Every format in the registry can be extracted (docs/05 §4).
    pub fn can_extract(&self) -> bool {
        true
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Format {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|f| f.name() == s).ok_or(())
    }
}

/// Context handed to [`FormatHandler`] operations: cancellation flag plus the
/// progress sink. Handlers must call [`HandlerContext::check_cancelled`]
/// between entries and report [`ProgressEvent::Advanced`] per entry; the
/// engine owns `Started`/`Finished`/`Failed`.
pub struct HandlerContext<'a> {
    cancelled: &'a AtomicBool,
    reporter: &'a dyn Fn(ProgressEvent),
}

impl<'a> HandlerContext<'a> {
    pub fn new(cancelled: &'a AtomicBool, reporter: &'a dyn Fn(ProgressEvent)) -> Self {
        Self {
            cancelled,
            reporter,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn check_cancelled(&self) -> Result<(), SquashError> {
        if self.is_cancelled() {
            Err(SquashError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn report(&self, event: ProgressEvent) {
        (self.reporter)(event)
    }
}

/// One format implementation (e.g. the `zip` crate adapter). Handlers are
/// registered in [`FormatRegistry`]; they never bypass the core safety layer
/// ([`crate::safety`]) on the extraction path.
///
/// `create`/`extract` default to [`SquashError::UnsupportedFormat`] so
/// capability-only handlers (and future extract-only ones) stay one-liners.
pub trait FormatHandler: Send + Sync {
    fn format(&self) -> Format;
    fn can_extract(&self) -> bool;
    fn can_create(&self) -> bool;

    /// Create `job.destination` from `job.inputs` using `job.preset`.
    fn create(&self, _job: &Job, _ctx: &HandlerContext) -> Result<JobStats, SquashError> {
        Err(SquashError::UnsupportedFormat)
    }

    /// Extract `archive` into `dest_dir` (pre-layout destination; the
    /// handler applies the docs/03 F3 layout rule via [`crate::layout`]).
    fn extract(
        &self,
        _archive: &Path,
        _dest_dir: &Path,
        _ctx: &HandlerContext,
    ) -> Result<JobStats, SquashError> {
        Err(SquashError::UnsupportedFormat)
    }
}

/// Maps detection results → [`FormatHandler`]. [`FormatRegistry::new`]
/// pre-registers every handler implemented in this crate; use
/// [`FormatRegistry::empty`] for tests that need a blank slate.
#[derive(Default)]
pub struct FormatRegistry {
    handlers: Vec<Box<dyn FormatHandler>>,
}

impl FormatRegistry {
    /// Registry with all built-in handlers (Phase 2: zip + 7z + tar family).
    pub fn new() -> Self {
        let mut reg = Self::empty();
        crate::formats::register_builtin(&mut reg);
        reg
    }

    /// Registry with no handlers (tests, custom wiring).
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn register(&mut self, handler: Box<dyn FormatHandler>) {
        self.handlers.push(handler);
    }

    pub fn handler_for(&self, format: Format) -> Option<&dyn FormatHandler> {
        self.handlers
            .iter()
            .map(|h| h.as_ref())
            .find(|h| h.format() == format)
    }

    /// Detect a format from a path hint. Magic-bytes detection takes priority
    /// once it lands (follow-up); today this is extension matching.
    pub fn detect(&self, hint: &Path) -> Option<Format> {
        let name = hint.file_name()?.to_str()?.to_ascii_lowercase();
        Format::ALL
            .into_iter()
            .find(|f| f.extensions().iter().any(|ext| name.ends_with(ext)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_by_extension() {
        let reg = FormatRegistry::new();
        assert_eq!(reg.detect(Path::new("a/backup.zip")), Some(Format::Zip));
        assert_eq!(reg.detect(Path::new("photos.TAR.GZ")), Some(Format::TarGz));
        assert_eq!(reg.detect(Path::new("x.tgz")), Some(Format::TarGz));
        assert_eq!(reg.detect(Path::new("x.tar.zst")), Some(Format::TarZst));
        assert_eq!(reg.detect(Path::new("no-extension")), None);
    }

    #[test]
    fn capability_contract() {
        // docs/05 §4: create set is exactly the MVP compress list; rar never.
        assert_eq!(Format::CREATE_CAPABLE.len(), 4);
        assert!(!Format::Rar.can_create());
        assert!(!Format::TarBz2.can_create());
        assert!(!Format::TarXz.can_create());
        assert!(!Format::Gz.can_create());
        assert!(!Format::Xz.can_create());
        assert!(Format::Zst.can_extract());
        assert!(Format::ALL.iter().all(|f| f.can_extract()));
    }

    #[test]
    fn registry_register_and_lookup_skeleton() {
        struct Dummy;
        impl FormatHandler for Dummy {
            fn format(&self) -> Format {
                Format::Rar
            }
            fn can_extract(&self) -> bool {
                true
            }
            fn can_create(&self) -> bool {
                false
            }
        }
        let mut reg = FormatRegistry::empty();
        assert!(reg.handler_for(Format::Rar).is_none());
        reg.register(Box::new(Dummy));
        assert!(reg.handler_for(Format::Rar).is_some());
        assert!(reg.handler_for(Format::SevenZ).is_none());
    }

    #[test]
    fn new_registry_has_builtin_handlers() {
        let reg = FormatRegistry::new();
        // Phase 2: zip + 7z + tar family are implemented…
        for format in [
            Format::Zip,
            Format::SevenZ,
            Format::Tar,
            Format::TarGz,
            Format::TarBz2,
            Format::TarXz,
            Format::TarZst,
        ] {
            assert!(reg.handler_for(format).is_some(), "{format} missing");
        }
        // …rar ships behind its default cargo feature…
        #[cfg(feature = "rar")]
        assert!(reg.handler_for(Format::Rar).is_some());
        #[cfg(not(feature = "rar"))]
        assert!(reg.handler_for(Format::Rar).is_none());
        // …gz/xz/zst land in later tasks.
        for format in [Format::Gz, Format::Xz, Format::Zst] {
            assert!(reg.handler_for(format).is_none(), "{format} unexpected");
        }
    }

    #[test]
    fn serialized_names_match_data_model() {
        // docs/06 stores these strings in settings/history.
        let names: Vec<String> = Format::ALL
            .iter()
            .map(|f| serde_json::to_string(f).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "\"zip\"",
                "\"7z\"",
                "\"rar\"",
                "\"tar\"",
                "\"tar.gz\"",
                "\"tar.bz2\"",
                "\"tar.xz\"",
                "\"tar.zst\"",
                "\"gz\"",
                "\"xz\"",
                "\"zst\""
            ]
        );
    }
}
