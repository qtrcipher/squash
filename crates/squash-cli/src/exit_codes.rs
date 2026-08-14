//! Deterministic exit codes (docs/03 §4: "script once, run everywhere,
//! trust the exit code"). Distinct non-zero per failure class.
//!
//! Stability contract, same as the error-code taxonomy in squash-core.

/// Success.
pub const SUCCESS: i32 = 0;
/// Internal error / feature not yet implemented.
pub const INTERNAL: i32 = 1;
/// Bad usage (clap parse failures also exit 2 by convention).
pub const USAGE: i32 = 2;
/// Corrupt or unreadable archive.
pub const CORRUPT: i32 = 3;
/// Password-protected / encrypted archive (stubbed, out of scope v1).
pub const ENCRYPTED: i32 = 4;
/// I/O failures including disk full and permission denied.
pub const IO: i32 = 5;
/// Unsafe archive (path traversal / zip-slip / decompression-bomb guard).
pub const UNSAFE_PATH: i32 = 6;
/// Cancelled by the user.
pub const CANCELLED: i32 = 7;

#[cfg(test)]
mod tests {
    use super::*;

    /// The exit-code set is a stability contract (docs/03 §4): 0 success,
    /// distinct non-zero per failure class.
    #[test]
    fn exit_codes_are_deterministic_and_distinct() {
        let codes = [
            SUCCESS,
            INTERNAL,
            USAGE,
            CORRUPT,
            ENCRYPTED,
            IO,
            UNSAFE_PATH,
            CANCELLED,
        ];
        assert_eq!(SUCCESS, 0);
        assert!(codes[1..].iter().all(|&c| c > 0));
        let unique: std::collections::BTreeSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len());
    }
}
