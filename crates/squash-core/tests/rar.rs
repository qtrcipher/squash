//! Integration: rar extraction (extract-only, `rar` feature — docs/05 §4).
//!
//! Fixtures are static files in `fixtures/` (real RARLAB-produced archives;
//! provenance in fixtures/README.md) because no `rar`/`unrar` binary exists
//! on the dev machines and Squash can never create rar itself. Tests skip
//! loudly if a fixture is absent.
//!
//! The zip-slip attack is crafted byte-level from the rar4 fixture: a file
//! header's stored name is swapped for a same-length `../` name and the
//! header CRC recomputed — like a real attack archive, the bytes are hostile
//! while the container stays structurally valid.

#![cfg(feature = "rar")]

use squash_core::{Engine, Format, Job, Preset, SquashError};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    if path.exists() {
        Some(path)
    } else {
        eprintln!("SKIP: fixtures/{name} missing (see fixtures/README.md)");
        None
    }
}

fn extract(archive: &Path, dest: &Path) -> Result<(), SquashError> {
    Engine::new()
        .submit(Job::extract(
            vec![archive.to_path_buf()],
            dest.to_path_buf(),
            Format::Rar,
        ))
        .wait()
        .map(|_| ())
}

/// The sharpcompress fixtures hold `тест.txt`, `exe\test.exe`,
/// `jpg\test.jpg` (plus an `Empty` dir in the rar5 one) — a loose root, so
/// extraction lands in `<dest>/<archive-stem>/` (docs/03 F3).
fn assert_sample_tree(dest: &Path, stem: &str) {
    let root = dest.join(stem);
    let size = |rel: &str| fs::metadata(root.join(rel)).map(|m| m.len()).ok();
    assert_eq!(
        size("тест.txt"),
        Some(15498),
        "non-ASCII entry must decode to the right name and size"
    );
    assert_eq!(size("exe/test.exe"), Some(45056));
    assert_eq!(size("jpg/test.jpg"), Some(40372));
}

#[test]
fn extract_rar4_fixture() {
    let Some(archive) = fixture("rar4-sample.rar") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    extract(&archive, tmp.path()).unwrap();
    assert_sample_tree(tmp.path(), "rar4-sample");
}

#[test]
fn extract_rar5_fixture() {
    let Some(archive) = fixture("rar5-sample.rar") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    extract(&archive, tmp.path()).unwrap();
    assert_sample_tree(tmp.path(), "rar5-sample");
    assert!(tmp.path().join("rar5-sample/Empty").is_dir());
}

#[test]
fn encrypted_header_rar_reports_password_required() {
    let Some(archive) = fixture("rar5-encrypted-header.rar") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let err = extract(&archive, tmp.path()).unwrap_err();
    assert_eq!(err, SquashError::PasswordRequired);
}

#[test]
fn fuzz_regression_null_flush_reports_corrupt_not_panic() {
    // Phase 5 fuzz finding (fuzz_rar, 2026-08-14): this input drives UnRAR
    // into a zero-length flush with a null data pointer, and the unrar-sys
    // trampoline passed it to `slice::from_raw_parts` — UB even at size 0,
    // caught by the fuzzer's unsafe-precondition checks. Fixed in unrar-sys
    // (null+0 chunks are skipped); the archive must now fail as corrupt.
    let Some(archive) = fixture("rar4-null-flush-regression.rar") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let err = extract(&archive, tmp.path()).unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn truncated_rar_reports_corrupt_archive() {
    let Some(archive) = fixture("rar5-sample.rar") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let bytes = fs::read(&archive).unwrap();
    let broken = tmp.path().join("truncated.rar");
    fs::write(&broken, &bytes[..bytes.len() / 2]).unwrap();

    let err = extract(&broken, &tmp.path().join("dest")).unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn garbage_rar_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let broken = tmp.path().join("broken.rar");
    // Valid rar5 signature, garbage where the headers should be.
    fs::write(
        &broken,
        b"Rar!\x1a\x07\x01\x00this is not a real rar archive",
    )
    .unwrap();

    let err = extract(&broken, &tmp.path().join("dest")).unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn rar_slip_aborts_with_path_traversal_blocked() {
    let Some(archive) = fixture("rar4-sample.rar") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let attack = tmp.path().join("attack.rar");
    craft_rar_slip(&archive, &attack);
    let dest = tmp.path().join("dest");

    let err = extract(&attack, &dest).unwrap_err();
    assert_eq!(err, SquashError::PathTraversalBlocked);
    assert!(
        !tmp.path().join("evil2.txt").exists(),
        "rar-slip payload escaped the destination"
    );
}

#[test]
fn compress_to_rar_is_unsupported() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), b"a").unwrap();
    let dest = tmp.path().join("out.rar");
    let err = Engine::new()
        .submit(Job::compress(
            vec![src],
            dest.clone(),
            Format::Rar,
            Preset::Balanced,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::UnsupportedFormat);
    assert!(!dest.exists());
}

// --- crafted slip archive ----------------------------------------------------

/// Rewrite the first file header's stored name to a same-length `../` name
/// and fix the header CRC. RAR4 headers: crc(u16) type(u8) flags(u16)
/// size(u16) …, file headers (type 0x74) carry pack_size(u32) after the
/// common part; the stored name sits at offset 32 within the header (25 for
/// the fixed file fields + 7 common, plus 8 when LHD_LARGE set — the fixture
/// uses neither LHD_LARGE nor unicode names on its first entry).
fn craft_rar_slip(source: &Path, out: &Path) {
    let mut bytes = fs::read(source).unwrap();
    assert_eq!(&bytes[..7], b"Rar!\x1a\x07\x00", "fixture must be rar4");

    let mut pos = 7usize;
    let patched = loop {
        let htype = bytes[pos + 2];
        let hsize = u16::from_le_bytes([bytes[pos + 5], bytes[pos + 6]]) as usize;
        if htype == 0x74 {
            let name_off = pos + 32;
            let nsize = u16::from_le_bytes([bytes[pos + 26], bytes[pos + 27]]) as usize;
            let old = String::from_utf8_lossy(&bytes[name_off..name_off + nsize]).into_owned();
            // Same length, so no sizes move: 12 bytes for `exe\test.exe`.
            let evil = b"../evil2.txt";
            assert_eq!(nsize, evil.len(), "fixture layout changed: {old}");
            bytes[name_off..name_off + nsize].copy_from_slice(evil);
            // HEAD_CRC = low 16 bits of CRC32 over the header after the crc.
            let crc = crc32(&bytes[pos + 2..pos + hsize]);
            bytes[pos..pos + 2].copy_from_slice(&(crc as u16).to_le_bytes());
            break true;
        }
        pos += hsize;
        if pos + 7 > bytes.len() {
            break false;
        }
    };
    assert!(patched, "no file header found in fixture");
    fs::write(out, bytes).unwrap();
}

/// CRC-32 (IEEE 802.3, poly 0xEDB88320) as RAR4 header checksums use.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
