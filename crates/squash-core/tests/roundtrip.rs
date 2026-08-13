//! Integration: per-format round-trips through the real engine (docs/05 §6,
//! 20% layer). Source trees include nested dirs and Arabic/Unicode names;
//! extraction results are byte-compared against the source.

mod common;

use common::{assert_trees_equal, build_source_tree};
use squash_core::{Engine, Format, Job, Preset};
use std::fs;
use std::path::Path;

fn roundtrip(format: Format, preset: Preset) {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    build_source_tree(&src);

    let engine = Engine::new();
    let archive = tmp.path().join(format!("out.{}", format.extensions()[0]));
    let stats = engine
        .submit(Job::compress(
            vec![src.join("data")],
            archive.clone(),
            format,
            preset,
        ))
        .wait()
        .unwrap_or_else(|e| panic!("compress {format}/{preset:?} failed: {e}"));
    assert_eq!(
        stats.in_bytes,
        11 + 256 + common::ARABIC_CONTENT.len() as u64
    );
    assert!(stats.out_bytes > 0);

    let dest = tmp.path().join("extracted");
    engine
        .submit(Job::extract(vec![archive], dest.clone(), format))
        .wait()
        .unwrap_or_else(|e| panic!("extract {format} failed: {e}"));

    // Single-root archive → extracts as-is (docs/03 F3).
    assert_trees_equal(&src.join("data"), &dest.join("data"));
}

#[test]
fn roundtrip_zip_all_presets() {
    for preset in Preset::ALL {
        roundtrip(Format::Zip, preset);
    }
}

#[test]
fn roundtrip_sevenz_all_presets() {
    for preset in Preset::ALL {
        roundtrip(Format::SevenZ, preset);
    }
}

/// Extract a 7z produced directly by the provider crate (not by the Squash
/// handler) — the foreign-archive path. TODO: no `7zz`/`7z` binary exists on
/// the dev Mac (`which 7zz 7z` is empty), so an upstream-7-Zip-produced
/// static fixture belongs in `fixtures/` once one can be captured.
#[test]
fn extract_sevenz_foreign() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    build_source_tree(&src);
    let archive = tmp.path().join("foreign.7z");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut writer = sevenz_rust2::ArchiveWriter::new(file).unwrap();
        for item in walkdir::WalkDir::new(src.join("data")).follow_links(false) {
            let item = item.unwrap();
            let name = item
                .path()
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if item.file_type().is_dir() {
                writer
                    .push_archive_entry(
                        sevenz_rust2::ArchiveEntry::new_directory(&name),
                        None::<&[u8]>,
                    )
                    .unwrap();
            } else {
                writer
                    .push_archive_entry(
                        sevenz_rust2::ArchiveEntry::new_file(&name),
                        Some(fs::File::open(item.path()).unwrap()),
                    )
                    .unwrap();
            }
        }
        writer.finish().unwrap();
    }

    let dest = tmp.path().join("extracted");
    Engine::new()
        .submit(Job::extract(vec![archive], dest.clone(), Format::SevenZ))
        .wait()
        .unwrap_or_else(|e| panic!("extract 7z failed: {e}"));
    assert_trees_equal(&src.join("data"), &dest.join("data"));
}

#[test]
fn loose_root_sevenz_extracts_into_archive_named_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("loose.7z");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut writer = sevenz_rust2::ArchiveWriter::new(file).unwrap();
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("a.txt"),
                Some(&b"aaa"[..]),
            )
            .unwrap();
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("b.txt"),
                Some(&b"bbb"[..]),
            )
            .unwrap();
        writer.finish().unwrap();
    }
    let dest = tmp.path().join("dest");
    Engine::new()
        .submit(Job::extract(vec![archive], dest.clone(), Format::SevenZ))
        .wait()
        .unwrap();
    // docs/03 F3: loose files → new folder named after the archive.
    assert_eq!(fs::read(dest.join("loose/a.txt")).unwrap(), b"aaa");
    assert_eq!(fs::read(dest.join("loose/b.txt")).unwrap(), b"bbb");
}

#[test]
fn sevenz_progress_events_flow() {
    use squash_core::ProgressEvent;
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    build_source_tree(&src);
    let engine = Engine::new();
    let archive = tmp.path().join("events.7z");
    let handle = engine.submit(Job::compress(
        vec![src.join("data")],
        archive.clone(),
        Format::SevenZ,
        Preset::Balanced,
    ));
    let mut events = Vec::new();
    while let Some(event) = handle.next_event() {
        events.push(event);
    }
    assert!(matches!(
        events.first(),
        Some(ProgressEvent::Started { .. })
    ));
    assert!(matches!(
        events.last(),
        Some(ProgressEvent::Finished { .. })
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, ProgressEvent::Advanced { .. })));

    // Extraction reports per-entry progress too.
    let handle = engine.submit(Job::extract(
        vec![archive],
        tmp.path().join("out"),
        Format::SevenZ,
    ));
    let mut events = Vec::new();
    while let Some(event) = handle.next_event() {
        events.push(event);
    }
    assert!(matches!(
        events.first(),
        Some(ProgressEvent::Started { .. })
    ));
    assert!(matches!(
        events.last(),
        Some(ProgressEvent::Finished { .. })
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, ProgressEvent::Advanced { .. })));
}

#[test]
fn roundtrip_tar_gz_all_presets() {
    for preset in Preset::ALL {
        roundtrip(Format::TarGz, preset);
    }
}

#[test]
fn roundtrip_tar_zst_all_presets() {
    for preset in Preset::ALL {
        roundtrip(Format::TarZst, preset);
    }
}

/// Build a single-root tar-family archive directly (the extract-only formats
/// have no Squash create path) using the same codec crates.
fn make_archive(archive: &Path, format: Format, src: &Path) {
    let file = fs::File::create(archive).unwrap();
    match format {
        Format::Tar => {
            let mut b = tar::Builder::new(file);
            b.append_dir_all("data", src).unwrap();
            b.finish().unwrap();
        }
        Format::TarGz => {
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::new(6));
            let mut b = tar::Builder::new(enc);
            b.append_dir_all("data", src).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        Format::TarBz2 => {
            let enc = bzip2::write::BzEncoder::new(file, bzip2::Compression::best());
            let mut b = tar::Builder::new(enc);
            b.append_dir_all("data", src).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        Format::TarXz => {
            let enc = xz2::write::XzEncoder::new(file, 6);
            let mut b = tar::Builder::new(enc);
            b.append_dir_all("data", src).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        other => panic!("make_archive: unexpected {other}"),
    }
}

fn extract_only_roundtrip(format: Format) {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    build_source_tree(&src);
    let archive = tmp
        .path()
        .join(format!("fixture.{}", format.extensions()[0]));
    make_archive(&archive, format, &src.join("data"));

    let dest = tmp.path().join("extracted");
    Engine::new()
        .submit(Job::extract(vec![archive], dest.clone(), format))
        .wait()
        .unwrap_or_else(|e| panic!("extract {format} failed: {e}"));
    assert_trees_equal(&src.join("data"), &dest.join("data"));
}

#[test]
fn extract_plain_tar() {
    extract_only_roundtrip(Format::Tar);
}

#[test]
fn extract_tar_bz2() {
    extract_only_roundtrip(Format::TarBz2);
}

#[test]
fn extract_tar_xz() {
    extract_only_roundtrip(Format::TarXz);
}

// Also exercise tar.gz extraction via a foreign-produced archive (the
// round-trip above is Squash→Squash; this one is flate2→Squash).
#[test]
fn extract_tar_gz_foreign() {
    extract_only_roundtrip(Format::TarGz);
}

#[test]
fn loose_root_zip_extracts_into_archive_named_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("loose.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("a.txt", opts).unwrap();
        std::io::Write::write_all(&mut zip, b"aaa").unwrap();
        zip.start_file("b.txt", opts).unwrap();
        std::io::Write::write_all(&mut zip, b"bbb").unwrap();
        zip.finish().unwrap();
    }
    let dest = tmp.path().join("dest");
    Engine::new()
        .submit(Job::extract(vec![archive], dest.clone(), Format::Zip))
        .wait()
        .unwrap();
    // docs/03 F3: loose files → new folder named after the archive.
    assert_eq!(fs::read(dest.join("loose/a.txt")).unwrap(), b"aaa");
    assert_eq!(fs::read(dest.join("loose/b.txt")).unwrap(), b"bbb");
}

#[test]
fn multiple_compress_inputs_keep_their_names() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("alpha");
    let b = tmp.path().join("beta.txt");
    fs::create_dir_all(&a).unwrap();
    fs::write(a.join("x.txt"), b"x").unwrap();
    fs::write(&b, b"beta").unwrap();

    let engine = Engine::new();
    let archive = tmp.path().join("multi.zip");
    engine
        .submit(Job::compress(
            vec![a, b],
            archive.clone(),
            Format::Zip,
            Preset::Balanced,
        ))
        .wait()
        .unwrap();
    let dest = tmp.path().join("out");
    engine
        .submit(Job::extract(vec![archive], dest.clone(), Format::Zip))
        .wait()
        .unwrap();
    // Two roots → loose layout under the archive stem.
    assert_eq!(fs::read(dest.join("multi/alpha/x.txt")).unwrap(), b"x");
    assert_eq!(fs::read(dest.join("multi/beta.txt")).unwrap(), b"beta");
}
