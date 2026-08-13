//! Builds the vendored RARLAB UnRAR C++ source (vendor/unrar) plus the small
//! C ABI shim in `shim/`.
//!
//! The compiled file list mirrors the upstream makefile's `lib` target
//! (`OBJECTS` + `filestr scantree dll qopen`) — see
//! vendor/unrar/README.squash.md. Files like `unpack15.cpp` or `crypt1.cpp`
//! are `#include`d from other translation units and must NOT be compiled
//! separately. `RARDLL` selects the dll.hpp API and (via os.hpp) implies
//! `SILENT`, so the library never prints to the console.

use std::path::PathBuf;

/// `OBJECTS` from vendor/unrar/makefile (order preserved).
const OBJECTS: &[&str] = &[
    "rar",
    "strlist",
    "strfn",
    "pathfn",
    "smallfn",
    "global",
    "file",
    "filefn",
    "filcreat",
    "archive",
    "arcread",
    "unicode",
    "system",
    "crypt",
    "crc",
    "rawread",
    "encname",
    "resource",
    "match",
    "timefn",
    "rdwrfn",
    "consio",
    "options",
    "errhnd",
    "rarvm",
    "secpassword",
    "rijndael",
    "getbits",
    "sha1",
    "sha256",
    "blake2s",
    "hash",
    "extinfo",
    "extract",
    "volume",
    "list",
    "find",
    "unpack",
    "headers",
    "threadpool",
    "rs16",
    "cmddata",
    "ui",
    "largepage",
];

/// `LIB_OBJ` from the makefile, minus the OBJECTS overlap.
const LIB_ONLY: &[&str] = &["filestr", "scantree", "dll", "qopen"];

fn main() {
    let vendor = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../vendor/unrar")
        .canonicalize()
        .expect("vendor/unrar must exist (see vendor/unrar/README.squash.md)");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        // Vendored third-party C++: not our warning surface, and some
        // compilers warn loudly on it.
        .warnings(false)
        .include(&vendor)
        // RARDLL selects the dll.hpp exports; os.hpp then defines SILENT.
        .define("RARDLL", None)
        // Upstream makefile DEFINES, unix-only (MSVC has no such knobs).
        .define("_FILE_OFFSET_BITS", "64")
        .define("_LARGEFILE_SOURCE", None);

    for unit in OBJECTS.iter().chain(LIB_ONLY) {
        build.file(vendor.join(format!("{unit}.cpp")));
    }
    build.file("shim/shim.cpp");
    build.compile("squash_unrar");

    // Rebuild when the vendored tree or the shim changes.
    println!("cargo:rerun-if-changed=shim/shim.cpp");
    println!("cargo:rerun-if-changed=shim/shim.h");
    println!("cargo:rerun-if-changed={}", vendor.display());
}
