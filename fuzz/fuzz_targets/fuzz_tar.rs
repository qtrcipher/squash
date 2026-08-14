#![no_main]
use libfuzzer_sys::fuzz_target;
use squash_core::Format;

// Raw tar bytes — the same decode path tar.bz2/tar.xz streams land on after
// decompression, so this covers the whole tar family's container parser.
fuzz_target!(|data: &[u8]| {
    squash_fuzz::drive(Format::Tar, "tar", data);
});
