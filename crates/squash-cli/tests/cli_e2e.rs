//! CLI end-to-end tests against the built `squash` binary (docs/05 §6 CLI
//! contract tests: exit codes, `--json` schema, round-trip through the
//! process boundary).

use assert_cmd::Command;
use std::fs;
use std::path::Path;

fn squash() -> Command {
    Command::cargo_bin("squash").unwrap()
}

/// Same canonical tree as the core fixtures: nested + Arabic names.
fn build_tree(root: &Path) {
    let data = root.join("data");
    fs::create_dir_all(data.join("nested")).unwrap();
    fs::create_dir_all(data.join("مجلد عربي")).unwrap();
    fs::write(data.join("hello.txt"), b"hello world").unwrap();
    fs::write(
        data.join("nested/deep.bin"),
        (0u16..=255).map(|b| b as u8).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(data.join("مجلد عربي/ملف.txt"), "محتوى عربي").unwrap();
}

#[test]
fn compress_then_extract_roundtrip_human_mode() {
    let tmp = tempfile::tempdir().unwrap();
    build_tree(tmp.path());
    let archive = tmp.path().join("out.tar.zst");

    squash()
        .args(["c", "data", "-o"])
        .arg(&archive)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("out.tar.zst —"))
        // Tiny trees are incompressible; the line must report either way.
        .stdout(predicates::str::is_match("(saved|grew)").unwrap());

    let dest = tmp.path().join("dest");
    squash()
        .args(["x"])
        .arg(&archive)
        .args(["-o"])
        .arg(&dest)
        .assert()
        .success()
        .stdout(predicates::str::contains("extracted to"));

    assert_eq!(
        fs::read(dest.join("data/hello.txt")).unwrap(),
        b"hello world"
    );
    assert_eq!(
        fs::read(dest.join("data/مجلد عربي/ملف.txt")).unwrap(),
        "محتوى عربي".as_bytes()
    );
}

#[test]
fn json_mode_emits_the_documented_schema() {
    let tmp = tempfile::tempdir().unwrap();
    build_tree(tmp.path());
    let archive = tmp.path().join("out.zip");

    let output = squash()
        .args(["--json", "c", "data", "-o"])
        .arg(&archive)
        .current_dir(tmp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lines: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    assert_eq!(lines[0]["type"], "job");
    assert_eq!(lines[0]["operation"], "compress");
    assert_eq!(lines[0]["format"], "zip");
    assert_eq!(lines[0]["preset"], "balanced");
    assert_eq!(lines[1]["type"], "started");
    assert!(lines[1]["total_bytes_estimate"].is_number());
    assert!(lines.iter().any(|l| l["type"] == "progress"));
    let result = lines.last().unwrap();
    assert_eq!(result["type"], "result");
    assert_eq!(result["status"], "finished");
    assert!(result["in_bytes"].is_number());
    assert!(result["out_bytes"].is_number());
}

#[test]
fn json_extract_result_mirrors_job_model() {
    let tmp = tempfile::tempdir().unwrap();
    build_tree(tmp.path());
    let archive = tmp.path().join("out.zip");
    squash()
        .args(["c", "data", "-o"])
        .arg(&archive)
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = squash()
        .args(["--json", "x"])
        .arg(&archive)
        .args(["-o"])
        .arg(tmp.path().join("dest"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lines: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["operation"], "extract");
    assert_eq!(lines.last().unwrap()["status"], "finished");
}

#[test]
fn corrupt_archive_exits_3_with_stable_error_code() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("broken.zip");
    fs::write(&archive, b"PK\x03\x04 not a real zip").unwrap();

    let assert = squash()
        .args(["--json", "x"])
        .arg(&archive)
        .args(["-o"])
        .arg(tmp.path().join("dest"))
        .assert()
        .code(3);
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: serde_json::Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(result["error"], "corrupt_archive");
}

#[test]
fn unsupported_compress_format_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    build_tree(tmp.path());
    // 7z is in the enum but has no handler yet (later task).
    squash()
        .args(["c", "data", "-f", "7z"])
        .current_dir(tmp.path())
        .assert()
        .code(2)
        .stderr(predicates::str::contains("cannot create"));
}

#[test]
fn undetectable_extract_format_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("mystery.bin");
    fs::write(&archive, b"whatever").unwrap();
    squash()
        .args(["x"])
        .arg(&archive)
        .assert()
        .code(2)
        .stderr(predicates::str::contains("cannot determine the format"));
}

#[test]
fn missing_input_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    squash()
        .args(["c", "no-such-dir", "-o", "out.zip"])
        .current_dir(tmp.path())
        .assert()
        .code(2)
        .stderr(predicates::str::contains("input not found"));
}

#[test]
fn zip_slip_archive_exits_6() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("attack.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("../../evil.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"evil").unwrap();
        zip.finish().unwrap();
    }
    squash()
        .args(["x"])
        .arg(&archive)
        .args(["-o"])
        .arg(tmp.path().join("dest"))
        .assert()
        .code(6)
        .stderr(predicates::str::contains("outside the destination"));
    assert!(!tmp.path().join("evil.txt").exists());
}

#[test]
fn extract_default_destination_is_archive_folder() {
    let tmp = tempfile::tempdir().unwrap();
    build_tree(tmp.path());
    squash()
        .args(["c", "data"])
        .current_dir(tmp.path())
        .assert()
        .success();
    // default output: data.zip next to the input
    assert!(tmp.path().join("data.zip").exists());
    squash()
        .args(["x", "data.zip"])
        .current_dir(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join("data/hello.txt").exists());
}
