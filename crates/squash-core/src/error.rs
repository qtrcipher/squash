//! Error taxonomy (docs/05 §3).
//!
//! One `SquashError` enum with **stable machine-readable codes**. Codes are a
//! stability contract — they are serialized into CLI `--json` output and
//! persisted in `history.jsonl` (`error_code`, docs/06 §2), and may only
//! change on a major version.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SquashError {
    #[error("unsupported format")]
    UnsupportedFormat,
    #[error("corrupt archive")]
    CorruptArchive,
    #[error("path traversal blocked")]
    PathTraversalBlocked,
    #[error("permission denied")]
    PermissionDenied,
    #[error("disk full")]
    DiskFull,
    /// Stubbed — encrypted archives are out of scope for v1 (docs/03 D2).
    #[error("password required")]
    PasswordRequired,
    #[error("cancelled")]
    Cancelled,
    #[error("internal error")]
    Internal,
}

impl SquashError {
    /// Stable machine-readable code (snake_case). Stability contract, majors only.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedFormat => "unsupported_format",
            Self::CorruptArchive => "corrupt_archive",
            Self::PathTraversalBlocked => "path_traversal_blocked",
            Self::PermissionDenied => "permission_denied",
            Self::DiskFull => "disk_full",
            Self::PasswordRequired => "password_required",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal",
        }
    }

    /// All variants, for exhaustive tests.
    pub const ALL: [SquashError; 8] = [
        Self::UnsupportedFormat,
        Self::CorruptArchive,
        Self::PathTraversalBlocked,
        Self::PermissionDenied,
        Self::DiskFull,
        Self::PasswordRequired,
        Self::Cancelled,
        Self::Internal,
    ];
}

/// Serialized form is the stable code string (docs/06 §2: "messages are
/// re-localized at render, never stored").
impl Serialize for SquashError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for SquashError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Self::ALL
            .into_iter()
            .find(|e| e.code() == code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown error code: {code}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Error codes are a stability contract — this test must not change
    /// except on a major version bump.
    #[test]
    fn error_codes_are_stable() {
        let expected = [
            (SquashError::UnsupportedFormat, "unsupported_format"),
            (SquashError::CorruptArchive, "corrupt_archive"),
            (SquashError::PathTraversalBlocked, "path_traversal_blocked"),
            (SquashError::PermissionDenied, "permission_denied"),
            (SquashError::DiskFull, "disk_full"),
            (SquashError::PasswordRequired, "password_required"),
            (SquashError::Cancelled, "cancelled"),
            (SquashError::Internal, "internal"),
        ];
        assert_eq!(SquashError::ALL.len(), expected.len());
        for (err, code) in expected {
            assert_eq!(err.code(), code);
        }
    }

    #[test]
    fn codes_are_unique() {
        let mut codes: Vec<_> = SquashError::ALL.iter().map(|e| e.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), SquashError::ALL.len());
    }

    #[test]
    fn codes_serialize_as_strings() {
        for err in SquashError::ALL {
            let json = serde_json::to_string(&err).unwrap();
            assert_eq!(json, format!("\"{}\"", err.code()));
            let back: SquashError = serde_json::from_str(&json).unwrap();
            assert_eq!(back, err);
        }
    }
}
