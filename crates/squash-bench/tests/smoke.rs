//! Smoke integration test: a tiny generated corpus driven through the
//! squash-only benchmark path (real Engine jobs, real archives on disk).

use squash_bench::cli::{run_benchmark, BenchOptions};
use squash_bench::{corpus, report};
use squash_core::format::Format;
use squash_core::presets::Preset;

#[test]
fn tiny_corpus_through_squash_only_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus_dir = tmp.path().join("corpus");
    let manifest = corpus::generate(&corpus_dir, 42, 0.001).unwrap();
    assert!(manifest.total_bytes > 0);

    let opts = BenchOptions {
        corpus_dir: corpus_dir.clone(),
        sets: vec!["text".to_string(), "small-files".to_string()],
        formats: vec![Format::Zip, Format::TarZst],
        presets: vec![Preset::Fast],
        reps: 1,
        warmup: 0,
        work_dir: tmp.path().join("work"),
        with_competitors: false,
    };
    let run = run_benchmark(&opts).unwrap();

    // 2 sets × 2 formats × 1 preset, all squash.
    assert_eq!(run.results.len(), 4);
    assert!(run.results.iter().all(|r| r.tool == "squash"));
    assert!(run
        .results
        .iter()
        .all(|r| r.in_bytes > 0 && r.out_bytes > 0));
    assert!(run.skipped.is_empty(), "squash-only mode skips nothing");

    // The text set is highly compressible; the harness must reflect that.
    let text_zip = run.results_for("squash", "zip", "fast", "text").unwrap();
    assert!(
        text_zip.ratio() < 1.0,
        "text zip ratio {}",
        text_zip.ratio()
    );

    // Run JSON round-trips (schema stability for --json / baseline.json).
    let json = serde_json::to_string_pretty(&run).unwrap();
    let parsed: squash_bench::model::BenchRun = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, run);

    // The markdown report renders every cell.
    let md = report::markdown(&run);
    for set in ["text", "small-files"] {
        assert!(md.contains(&format!("### set: {set}")));
    }
    assert!(md.matches("| squash |").count() >= 4);
}

#[test]
fn missing_manifest_is_a_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let opts = BenchOptions {
        corpus_dir: tmp.path().join("nope"),
        sets: vec![],
        formats: vec![Format::Zip],
        presets: vec![Preset::Fast],
        reps: 1,
        warmup: 0,
        work_dir: tmp.path().join("work"),
        with_competitors: false,
    };
    let err = run_benchmark(&opts).unwrap_err();
    assert!(err.contains("corpus generate"), "{err}");
}

#[test]
fn zero_reps_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus_dir = tmp.path().join("corpus");
    corpus::generate(&corpus_dir, 42, 0.001).unwrap();
    let opts = BenchOptions {
        corpus_dir,
        sets: vec!["text".to_string()],
        formats: vec![Format::Zip],
        presets: vec![Preset::Fast],
        reps: 0,
        warmup: 0,
        work_dir: tmp.path().join("work"),
        with_competitors: false,
    };
    assert!(run_benchmark(&opts).unwrap_err().contains("--reps"));
}
