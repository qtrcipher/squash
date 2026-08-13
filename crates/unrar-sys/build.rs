//! Builds the vendored RARLAB UnRAR C++ source (vendor/unrar) plus the small
//! C ABI shim in `shim/`.
//!
//! The compiled file list mirrors the upstream makefile's `lib` target
//! (`OBJECTS` + `filestr scantree dll qopen`) — see
//! vendor/unrar/README.squash.md. Files like `unpack15.cpp` or `crypt1.cpp`
//! are `#include`d from other translation units and must NOT be compiled
//! separately. `RARDLL` selects the dll.hpp API and (via os.hpp) implies
//! `SILENT`, so the library never prints to the console.

use std::path::{Path, PathBuf};

/// Strip a Windows `\\?\` verbatim prefix from a path.
///
/// `Path::canonicalize` returns verbatim paths on Windows (e.g.
/// `\\?\D:\a\squash\vendor\unrar`), and cl.exe does not accept those as
/// source-file arguments — the path gets mangled and every compile fails
/// with `c1xx: fatal error C1083: Cannot open source file: '\\rar.cpp'`.
/// The compiler must only ever see plain paths, so strip the prefix here.
/// No-op on plain paths and on non-Windows platforms.
fn strip_verbatim_prefix(p: &Path) -> PathBuf {
    let s = p.as_os_str().to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        // `\\?\UNC\server\share\...` -> `\\server\share\...`
        Some(rest) if rest.starts_with(r"UNC\") => PathBuf::from(format!(r"\\{}", &rest[4..])),
        Some(rest) => PathBuf::from(rest),
        None => p.to_path_buf(),
    }
}

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

/// Windows-only units the unix makefile `lib` list does not know about.
///
/// Taken from the authoritative Windows source list,
/// vendor/unrar/UnRARDll.vcxproj (`<ClCompile Include="isnt.cpp" />` and
/// `"motw.cpp"`). `isnt.cpp` defines `WinNT()` / `IsWindows11OrGreater()`
/// (used by extract.cpp, timefn.cpp, consio.cpp, pathfn.cpp, threadpool.cpp)
/// and `motw.cpp` defines the `MarkOfTheWeb::*` methods (Mark of the Web
/// propagation, used by extract.cpp) — without these the MSVC link fails
/// with unresolved externals. `ulinks.cpp` stays excluded: the vcxproj does
/// not list it and extinfo.cpp only `#include`s it under `_WIN_UNIX`.
/// `rs.cpp` is listed by the vcxproj but is not in the makefile `lib` list
/// and no unresolved symbol maps to it, so it is deliberately left out.
const WINDOWS_ONLY: &[&str] = &["isnt", "motw"];

fn main() {
    let vendor = strip_verbatim_prefix(
        &PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("../../vendor/unrar")
            .canonicalize()
            .expect("vendor/unrar must exist (see vendor/unrar/README.squash.md)"),
    );

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
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for unit in WINDOWS_ONLY {
            build.file(vendor.join(format!("{unit}.cpp")));
        }
    }
    build.file("shim/shim.cpp");
    build.compile("squash_unrar");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // pathfn.cpp calls SHGetSpecialFolderLocation / SHGetPathFromIDListW
        // (GetAppDataPath) — shell API with no `#pragma comment(lib)` in the
        // vendored source, so it must be linked explicitly. Shlwapi, PowrProf,
        // Psapi and wbemuuid pull themselves in via pragmas in os.hpp; motw.cpp
        // needs no extra libs (plain File I/O, no COM/urlmon).
        println!("cargo:rustc-link-lib=shell32");
    }

    // Rebuild when the vendored tree or the shim changes.
    println!("cargo:rerun-if-changed=shim/shim.cpp");
    println!("cargo:rerun-if-changed=shim/shim.h");
    println!("cargo:rerun-if-changed={}", vendor.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_drive_verbatim_prefix() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\D:\a\squash\squash\vendor\unrar")),
            PathBuf::from(r"D:\a\squash\squash\vendor\unrar")
        );
    }

    #[test]
    fn strips_unc_verbatim_prefix() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\UNC\server\share\vendor\unrar")),
            PathBuf::from(r"\\server\share\vendor\unrar")
        );
    }

    #[test]
    fn leaves_plain_paths_untouched() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"D:\a\squash\vendor\unrar")),
            PathBuf::from(r"D:\a\squash\vendor\unrar")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new("/Users/dev/squash/vendor/unrar")),
            PathBuf::from("/Users/dev/squash/vendor/unrar")
        );
    }
}
