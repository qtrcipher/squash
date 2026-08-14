#![no_main]
use libfuzzer_sys::fuzz_target;
use squash_core::Format;

// rar: the highest-value target — hostile bytes reach vendored UnRAR 7.2.7
// C++ (header parse, decompression VM) before any Rust guard runs
// (docs/07 §4). The shim runs UnRAR in RAR_TEST mode: it never touches the
// filesystem; only Squash writes, inside the per-run tempdir.
fuzz_target!(|data: &[u8]| {
    squash_fuzz::drive(Format::Rar, "rar", data);
});
