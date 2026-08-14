#![no_main]
use libfuzzer_sys::fuzz_target;
use squash_core::Format;

fuzz_target!(|data: &[u8]| {
    squash_fuzz::drive(Format::Zip, "zip", data);
});
