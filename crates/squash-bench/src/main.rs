//! Benchmark harness stub (docs/05 §6).
//!
//! Phase 2+: runs the standard corpus (`benches/corpus/`) through Squash and
//! `7zz`, records ratio + wall time, fails CI on regression > 2%.

fn main() {
    // Prove the core linkage; the harness itself is Phase 2.
    let _presets = squash_core::presets::PRESET_TABLE.len();
    println!("squash-bench: benchmark harness not yet implemented");
}
