//! Shared driver for the Squash extraction fuzz targets (docs/05 §6,
//! residual risks from docs/07 §4).
//!
//! Every target feeds libFuzzer's bytes to the **real** extraction entry
//! point — `FormatHandler::extract` — so the fuzzer covers the full parse →
//! layout → sanitize → guarded-decode path, not a trimmed reimplementation.
//! The handlers take archive paths, so each run writes the input into a
//! per-run tempdir and extracts into a sibling dir; both are deleted when
//! the tempdir drops. Nothing outside that tempdir is ever touched.
//!
//! The decompression-bomb guard (`squash_core::safety::ExtractGuard`) stays
//! active with tight env limits so bombs abort as `DecompressionBomb`
//! instead of OOM-ing or hanging the fuzzer: 8 MiB expanded / 1024 entries
//! per run. The ratio cap is left at its default — the 8 MiB absolute cap
//! trips long before the 64 MiB ratio floor matters.

use squash_core::format::FormatRegistry;
use squash_core::{Format, HandlerContext};
use std::sync::atomic::AtomicBool;
use std::sync::Once;

static CANCELLED: AtomicBool = AtomicBool::new(false);
static LIMITS: Once = Once::new();

fn set_tight_limits() {
    LIMITS.call_once(|| {
        // libFuzzer runs single-threaded per process; setting these once at
        // startup is safe. Unparseable values fail safe (defaults), but
        // these parse.
        std::env::set_var("SQUASH_EXTRACT_MAX_BYTES", "8388608"); // 8 MiB
        std::env::set_var("SQUASH_EXTRACT_MAX_ENTRIES", "1024");
    });
}

/// Drive one extraction: `data` as an archive named `input.<ext>`, extracted
/// with the handler for `format` into a fresh tempdir. All outcomes are
/// in-bounds — corrupt input must surface as `Err`, never a panic (a panic
/// IS the fuzz finding); success is fine too (valid archives occur).
pub fn drive(format: Format, ext: &str, data: &[u8]) {
    set_tight_limits();
    let Ok(run) = tempfile::tempdir() else {
        return;
    };
    let archive = run.path().join(format!("input.{ext}"));
    if std::fs::write(&archive, data).is_err() {
        return;
    }
    let dest = run.path().join("out");
    let registry = FormatRegistry::new();
    let Some(handler) = registry.handler_for(format) else {
        return;
    };
    let ctx = HandlerContext::new(&CANCELLED, &|_| {});
    let _ = handler.extract(&archive, &dest, &ctx);
}
