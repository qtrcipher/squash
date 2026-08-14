#![no_main]
use libfuzzer_sys::fuzz_target;
use squash_core::Format;

// Compressed container: gzip stream → tar parser → guarded extraction.
fuzz_target!(|data: &[u8]| {
    squash_fuzz::drive(Format::TarGz, "tar.gz", data);
});
