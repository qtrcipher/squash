#![no_main]
use libfuzzer_sys::fuzz_target;
use squash_core::Format;

// Single-file zst: one zstd stream decoded into one guarded output file.
fuzz_target!(|data: &[u8]| {
    squash_fuzz::drive(Format::Zst, "zst", data);
});
