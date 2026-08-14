#![no_main]
use libfuzzer_sys::fuzz_target;
use squash_core::Format;

// 7z: exercises sevenz-rust2's header parsing, including its allocations on
// declared header sizes (docs/07 finding 10 — a residual risk tracked here).
fuzz_target!(|data: &[u8]| {
    squash_fuzz::drive(Format::SevenZ, "7z", data);
});
