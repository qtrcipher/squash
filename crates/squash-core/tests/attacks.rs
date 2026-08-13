//! Integration: hostile and broken archives (docs/05 §6). Zip-slip archives
//! are crafted programmatically; every attack must abort the job with the
//! documented `SquashError` and leave nothing outside the destination.

use squash_core::{Engine, Format, Job, SquashError};
use std::fs;
use std::io::Write;
use std::path::Path;

fn engine() -> Engine {
    Engine::new()
}

// --- crafted archives --------------------------------------------------------

/// A zip containing `safe/ok.txt` plus `../../evil.txt`.
fn crafted_zip_slip(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("safe/ok.txt", opts).unwrap();
    zip.write_all(b"ok").unwrap();
    // The zip writer must not sanitize names for us — if a future zip
    // crate version starts rejecting this, switch to hand-rolled bytes.
    zip.start_file("../../evil.txt", opts)
        .expect("test requires writing raw traversal names");
    zip.write_all(b"evil").unwrap();
    zip.finish().unwrap();
}

/// A tar containing a `../evil.txt` entry. The name is injected into the raw
/// header because `tar`'s `append_data` refuses `..` (as it should) — real
/// attack archives are crafted byte-level, and so is this fixture.
fn crafted_tar_slip(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    let name = b"../evil.txt";
    header.as_old_mut().name[..name.len()].copy_from_slice(name);
    header.set_cksum();
    builder.append(&header, &b"evil"[..]).unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_size(2);
    header.set_cksum();
    builder
        .append_data(&mut header, "ok.txt", &b"ok"[..])
        .unwrap();
    builder.finish().unwrap();
}

/// A tar whose symlink points outside the destination.
fn crafted_tar_symlink_escape(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_cksum();
    builder
        .append_link(&mut header, "link", "/etc/passwd")
        .unwrap();
    builder.finish().unwrap();
}

/// A 7z containing `safe/ok.txt` plus `../../evil.txt`. The 7z format stores
/// names as plain UTF-16 strings and the writer does not sanitize them, so
/// the raw traversal name goes in as-is — like a real attack archive.
fn crafted_7z_slip(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut writer = sevenz_rust2::ArchiveWriter::new(file).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("safe/ok.txt"),
            Some(&b"ok"[..]),
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("../../evil.txt"),
            Some(&b"evil"[..]),
        )
        .unwrap();
    writer.finish().unwrap();
}

// --- tests -------------------------------------------------------------------

#[test]
fn zip_slip_aborts_with_path_traversal_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("attack.zip");
    crafted_zip_slip(&archive);
    let dest = tmp.path().join("dest");

    let err = engine()
        .submit(Job::extract(vec![archive], dest.clone(), Format::Zip))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::PathTraversalBlocked);
    assert!(
        !tmp.path().join("evil.txt").exists(),
        "zip-slip payload escaped the destination"
    );
}

#[test]
fn tar_slip_aborts_with_path_traversal_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("attack.tar");
    crafted_tar_slip(&archive);
    let dest = tmp.path().join("dest");

    let err = engine()
        .submit(Job::extract(vec![archive], dest.clone(), Format::Tar))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::PathTraversalBlocked);
    assert!(!tmp.path().join("evil.txt").exists());
}

#[test]
fn tar_symlink_escape_aborts_with_path_traversal_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("attack.tar");
    crafted_tar_symlink_escape(&archive);
    let dest = tmp.path().join("dest");

    let err = engine()
        .submit(Job::extract(vec![archive], dest.clone(), Format::Tar))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::PathTraversalBlocked);
    assert!(!dest.join("link").exists());
}

#[test]
fn sevenz_slip_aborts_with_path_traversal_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("attack.7z");
    crafted_7z_slip(&archive);
    let dest = tmp.path().join("dest");

    let err = engine()
        .submit(Job::extract(vec![archive], dest.clone(), Format::SevenZ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::PathTraversalBlocked);
    assert!(
        !tmp.path().join("evil.txt").exists(),
        "7z-slip payload escaped the destination"
    );
}

#[test]
fn corrupt_sevenz_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.7z");
    // Valid 7z signature, garbage where the header should be.
    fs::write(
        &archive,
        b"\x37\x7a\xbc\xaf\x27\x1c\x00\x04this is not a real 7z archive at all",
    )
    .unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::SevenZ,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn truncated_sevenz_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    // Build a valid 7z, then cut it in half (the 7z header sits at the end).
    let archive = tmp.path().join("truncated.7z");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut writer = sevenz_rust2::ArchiveWriter::new(file).unwrap();
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("a.txt"),
                Some(&b"data"[..]),
            )
            .unwrap();
        writer.finish().unwrap();
    }
    let bytes = fs::read(&archive).unwrap();
    fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::SevenZ,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn corrupt_zip_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.zip");
    fs::write(&archive, b"PK\x03\x04 this is not a real zip").unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::Zip,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn truncated_zip_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    // Build a valid zip, then cut it in half.
    let archive = tmp.path().join("truncated.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("a.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"data").unwrap();
        zip.finish().unwrap();
    }
    let bytes = fs::read(&archive).unwrap();
    fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::Zip,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn corrupt_tar_gz_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.tar.gz");
    // Valid gzip magic + header, garbage payload.
    fs::write(
        &archive,
        b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\xffgarbage-garbage-garbage",
    )
    .unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::TarGz,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn corrupt_tar_zst_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.tar.zst");
    fs::write(&archive, b"\x28\xb5\x2f\xfdnot-a-real-zstd-frame").unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::TarZst,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn failed_compress_leaves_no_partial_output() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.zip");
    // Missing input → job fails; docs/03 F2: partial output is deleted.
    let err = engine()
        .submit(Job::compress(
            vec![tmp.path().join("missing")],
            dest.clone(),
            Format::Zip,
            squash_core::Preset::Balanced,
        ))
        .wait()
        .unwrap_err();
    assert!(matches!(
        err,
        SquashError::Internal | SquashError::PermissionDenied
    ));
    assert!(!dest.exists());
}

// --- single-file codecs (gz / xz / zst) --------------------------------------

/// A valid `.zst` produced by the handler, for truncation fixtures.
fn valid_zst(path: &Path) {
    let tmp_src = path.with_extension("raw");
    fs::write(&tmp_src, b"single-file payload, soon to be truncated").unwrap();
    engine()
        .submit(Job::compress(
            vec![tmp_src.clone()],
            path.to_path_buf(),
            Format::Zst,
            squash_core::Preset::Balanced,
        ))
        .wait()
        .unwrap();
    fs::remove_file(&tmp_src).unwrap();
}

#[test]
fn corrupt_gz_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.gz");
    // Valid gzip magic + header, garbage payload.
    fs::write(
        &archive,
        b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\xffgarbage-garbage-garbage",
    )
    .unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::Gz,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn corrupt_xz_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.xz");
    // Valid xz stream magic, garbage where the block header should be.
    fs::write(&archive, b"\xfd7zXZ\x00not-a-real-xz-stream-at-all").unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::Xz,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn corrupt_zst_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.zst");
    fs::write(&archive, b"\x28\xb5\x2f\xfdnot-a-real-zstd-frame").unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::Zst,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn truncated_zst_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("truncated.zst");
    valid_zst(&archive);
    let bytes = fs::read(&archive).unwrap();
    fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::Zst,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}

#[test]
fn truncated_gz_reports_corrupt_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("truncated.gz");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::new(6));
        enc.write_all(b"single-file payload, soon to be truncated")
            .unwrap();
        enc.finish().unwrap();
    }
    let bytes = fs::read(&archive).unwrap();
    fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();

    let err = engine()
        .submit(Job::extract(
            vec![archive],
            tmp.path().join("dest"),
            Format::Gz,
        ))
        .wait()
        .unwrap_err();
    assert_eq!(err, SquashError::CorruptArchive);
}
