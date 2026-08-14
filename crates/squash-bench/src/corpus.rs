//! Standard benchmark corpus (`benches/corpus/`), generated deterministically.
//!
//! Everything derives from a seeded SplitMix64 stream (see [`crate::prng`]),
//! so `corpus generate --seed 42 --scale 1.0` produces byte-identical output
//! on every machine — the corpus is reproducible by construction, which is
//! why it stays out of git (only this generator and the README are
//! versioned). A `manifest.json` with per-set FNV-1a checksums is written
//! alongside the data so a corpus on disk can be verified against the spec.
//!
//! Composition at scale 1.0 (~245 MiB total, "a few hundred MB max"):
//!
//! | set | content | size | compressibility |
//! |---|---|---|---|
//! | `text/` | synthetic logs/source/CSV | 96 MiB | high |
//! | `binary/` | structured 64-byte records | 64 MiB | medium |
//! | `mixed/` | project-tree mix of the other kinds | ~48 MiB | mixed |
//! | `compressed/` | pure PRNG bytes | 32 MiB | ~none (already-compressed stand-in) |
//! | `small-files/` | 2048 tiny text files, nested | ~5 MiB | per-file overhead probe |

use crate::prng::{Fnv1a, SplitMix64};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

pub const DEFAULT_SEED: u64 = 42;
pub const SETS: [&str; 5] = ["text", "binary", "mixed", "compressed", "small-files"];

const MIB: u64 = 1024 * 1024;
const KIB: u64 = 1024;

/// Scale a byte target, keeping a floor so tiny scales (tests, smoke runs)
/// still produce real files.
fn scaled(bytes: u64, scale: f64, min: u64) -> u64 {
    (((bytes as f64) * scale) as u64).max(min)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetInfo {
    pub name: String,
    pub files: u64,
    pub bytes: u64,
    /// FNV-1a 64 over the concatenated file contents (hex).
    pub fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub seed: u64,
    pub scale: f64,
    pub total_bytes: u64,
    pub sets: Vec<SetInfo>,
}

/// Accumulates per-set stats while files are written.
struct SetAcc {
    files: u64,
    bytes: u64,
    hash: Fnv1a,
}

impl SetAcc {
    fn new() -> Self {
        Self {
            files: 0,
            bytes: 0,
            hash: Fnv1a::new(),
        }
    }

    fn record(&mut self, bytes: &[u8]) {
        self.files += 1;
        self.bytes += bytes.len() as u64;
        self.hash.update(bytes);
    }

    fn finish(self, name: &str) -> SetInfo {
        SetInfo {
            name: name.to_string(),
            files: self.files,
            bytes: self.bytes,
            fnv1a64: format!("{:016x}", self.hash.finish()),
        }
    }
}

/// Write one file, hashing its content into the set accumulator.
fn write_hashed(path: &Path, content: &[u8], acc: &mut SetAcc) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    acc.record(content);
    Ok(())
}

/// Generate the full corpus under `dir` (created if missing). Existing
/// set directories are replaced; unrelated files in `dir` are left alone.
pub fn generate(dir: &Path, seed: u64, scale: f64) -> io::Result<Manifest> {
    fs::create_dir_all(dir)?;
    let sets = vec![
        gen_text(dir, seed, scale)?,
        gen_binary(dir, seed, scale)?,
        gen_mixed(dir, seed, scale)?,
        gen_compressed(dir, seed, scale)?,
        gen_small_files(dir, seed, scale)?,
    ];
    let manifest = Manifest {
        version: 1,
        seed,
        scale,
        total_bytes: sets.iter().map(|s| s.bytes).sum(),
        sets,
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(io::Error::other)?;
    fs::write(dir.join("manifest.json"), json)?;
    Ok(manifest)
}

/// Read the manifest written by [`generate`], if present.
pub fn load_manifest(corpus_dir: &Path) -> io::Result<Option<Manifest>> {
    let path = corpus_dir.join("manifest.json");
    match fs::read_to_string(&path) {
        Ok(text) => {
            let manifest = serde_json::from_str(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(Some(manifest))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Total size of a directory tree (files only).
pub fn dir_size(path: &Path) -> io::Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            total += dir_size(&entry.path())?;
        } else if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

// --- text set ---------------------------------------------------------------

/// One synthetic text line from family `kind` (0..4): HTTP log, build log,
/// source code, CSV. Numbers come from the stream so content is varied but
/// highly repetitive — realistic compressibility for text workloads.
fn text_line(rng: &mut SplitMix64, kind: u64, out: &mut String) {
    use std::fmt::Write as _;
    match kind {
        0 => {
            let paths = [
                "api/v1/items",
                "api/v1/users",
                "api/v2/search",
                "static/app.js",
            ];
            let statuses = [200, 200, 200, 200, 301, 404, 500];
            let _ = writeln!(
                out,
                "2026-08-14T{:02}:{:02}:{:02}Z INFO  worker-{} request_id={:016x} method=GET path=/{} status={} duration_ms={}",
                rng.below(24),
                rng.below(60),
                rng.below(60),
                rng.below(16),
                rng.next_u64(),
                paths[rng.below(paths.len() as u64) as usize],
                statuses[rng.below(statuses.len() as u64) as usize],
                rng.below(900),
            );
        }
        1 => {
            let _ = writeln!(
                out,
                "[build] compiling crate_{} v1.{}.{} (target/aarch64-apple-darwin, opt-level=2) ... ok in {}.{:03}s",
                rng.below(400),
                rng.below(20),
                rng.below(10),
                rng.below(30),
                rng.below(1000),
            );
        }
        2 => {
            let _ = write!(
                out,
                "fn handle_request_{}(ctx: &mut Context, req: Request) -> Result<Response, Error> {{\n    let started = Instant::now();\n    let item_{} = ctx.store.load({})?;\n    tracing::info!(\"processed item {{}} in {{:?}}\", item_{}.id, started.elapsed());\n    Ok(Response::json(&item_{}))\n}}\n\n",
                rng.below(10_000),
                rng.below(10_000),
                rng.below(1_000_000),
                rng.below(10_000),
                rng.below(10_000),
            );
        }
        _ => {
            let names = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];
            let _ = writeln!(
                out,
                "{},{},{}.{:02},{},{},2026-{:02}-{:02}",
                1_000_000 + rng.below(9_000_000),
                names[rng.below(names.len() as u64) as usize],
                rng.below(10_000),
                rng.below(100),
                rng.below(500),
                rng.below(10_000),
                1 + rng.below(12),
                1 + rng.below(28),
            );
        }
    }
}

/// Build ~`target` bytes of synthetic text (exact size may overshoot by one
/// line; deterministic regardless).
fn text_blob(seed: u64, tag_a: u64, tag_b: u64, target: u64) -> Vec<u8> {
    let mut rng = SplitMix64::derive(seed, tag_a, tag_b);
    let mut out = String::new();
    while (out.len() as u64) < target {
        let kind = rng.below(4);
        text_line(&mut rng, kind, &mut out);
    }
    out.into_bytes()
}

/// Structured binary: 64-byte records (seq, kind, timestamp, flags, payload)
/// with zero-run payloads — medium compressibility, like a database page.
fn binary_blob(seed: u64, tag_a: u64, tag_b: u64, target: u64) -> Vec<u8> {
    let mut rng = SplitMix64::derive(seed, tag_a, tag_b);
    let records = target.div_ceil(64);
    let mut out = Vec::with_capacity((records * 64) as usize);
    let base_ts = 1_789_000_000u32;
    for seq in 0..records {
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(&(rng.below(8) as u32).to_le_bytes());
        out.extend_from_slice(&(base_ts + rng.below(86_400) as u32).to_le_bytes());
        let flags = if rng.below(4) == 0 {
            rng.next_u64() as u32
        } else {
            0
        };
        out.extend_from_slice(&flags.to_le_bytes());
        let mut payload = [0u8; 44];
        if rng.below(10) < 3 {
            rng.fill_bytes(&mut payload);
        }
        out.extend_from_slice(&payload);
    }
    out.truncate(target as usize);
    out
}

/// Pure PRNG bytes — stands in for already-compressed content.
fn random_blob(seed: u64, tag_a: u64, tag_b: u64, target: u64) -> Vec<u8> {
    let mut rng = SplitMix64::derive(seed, tag_a, tag_b);
    let mut out = vec![0u8; target as usize];
    rng.fill_bytes(&mut out);
    out
}

fn gen_text(dir: &Path, seed: u64, scale: f64) -> io::Result<SetInfo> {
    let set = "text";
    let mut acc = SetAcc::new();
    let per_file = scaled(12 * MIB, scale, 512);
    for i in 0..8u64 {
        let (sub, name) = if i % 2 == 0 {
            ("logs", format!("server_{i:02}.log"))
        } else {
            ("src", format!("module_{i:02}.rs"))
        };
        let blob = text_blob(seed, 0, i, per_file);
        write_hashed(&dir.join(set).join(sub).join(name), &blob, &mut acc)?;
    }
    Ok(acc.finish(set))
}

fn gen_binary(dir: &Path, seed: u64, scale: f64) -> io::Result<SetInfo> {
    let set = "binary";
    let mut acc = SetAcc::new();
    let per_file = scaled(16 * MIB, scale, 512);
    for i in 0..4u64 {
        let blob = binary_blob(seed, 1, i, per_file);
        write_hashed(
            &dir.join(set).join("db").join(format!("table_{i}.bin")),
            &blob,
            &mut acc,
        )?;
    }
    Ok(acc.finish(set))
}

fn gen_compressed(dir: &Path, seed: u64, scale: f64) -> io::Result<SetInfo> {
    let set = "compressed";
    let mut acc = SetAcc::new();
    let per_file = scaled(16 * MIB, scale, 512);
    for i in 0..2u64 {
        let blob = random_blob(seed, 3, i, per_file);
        write_hashed(
            &dir.join(set).join(format!("blob_{i}.dat")),
            &blob,
            &mut acc,
        )?;
    }
    Ok(acc.finish(set))
}

fn gen_mixed(dir: &Path, seed: u64, scale: f64) -> io::Result<SetInfo> {
    let set = "mixed";
    let mut acc = SetAcc::new();
    for i in 0..32u64 {
        let mut size_rng = SplitMix64::derive(seed, 2, i);
        let target = scaled(768 * KIB + size_rng.below(1536 * KIB), scale, 256);
        let (sub, name, blob) = match i % 3 {
            0 => (
                "docs",
                format!("doc_{i:02}.md"),
                text_blob(seed, 20, i, target),
            ),
            1 => (
                "src",
                format!("code_{i:02}.rs"),
                binary_blob(seed, 21, i, target),
            ),
            _ => (
                "assets",
                format!("asset_{i:02}.png"),
                random_blob(seed, 22, i, target),
            ),
        };
        write_hashed(&dir.join(set).join(sub).join(name), &blob, &mut acc)?;
    }
    Ok(acc.finish(set))
}

fn gen_small_files(dir: &Path, seed: u64, scale: f64) -> io::Result<SetInfo> {
    let set = "small-files";
    let mut acc = SetAcc::new();
    let count = scaled(2048, scale, 8);
    for i in 0..count {
        let mut size_rng = SplitMix64::derive(seed, 40, i);
        let target = scaled(KIB + size_rng.below(3 * KIB), scale, 128);
        let blob = text_blob(seed, 41, i, target);
        write_hashed(
            &dir.join(set)
                .join(format!("dir_{:02}", i % 16))
                .join(format!("note_{i:04}.txt")),
            &blob,
            &mut acc,
        )?;
    }
    Ok(acc.finish(set))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ma = generate(a.path(), 42, 0.001).unwrap();
        let mb = generate(b.path(), 42, 0.001).unwrap();
        assert_eq!(ma, mb, "same seed+scale must give identical manifests");
        assert!(ma.total_bytes > 0);
        assert_eq!(ma.sets.len(), SETS.len());
    }

    #[test]
    fn different_seed_changes_content() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ma = generate(a.path(), 1, 0.001).unwrap();
        let mb = generate(b.path(), 2, 0.001).unwrap();
        assert_ne!(ma.sets[0].fnv1a64, mb.sets[0].fnv1a64);
    }

    #[test]
    fn manifest_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let written = generate(dir.path(), 42, 0.001).unwrap();
        let loaded = load_manifest(dir.path()).unwrap().unwrap();
        assert_eq!(written, loaded);
        assert!(load_manifest(tempfile::tempdir().unwrap().path())
            .unwrap()
            .is_none());
    }

    #[test]
    fn scale_shrinks_output() {
        let big = tempfile::tempdir().unwrap();
        let small = tempfile::tempdir().unwrap();
        let mb = generate(big.path(), 42, 0.01).unwrap();
        let ms = generate(small.path(), 42, 0.001).unwrap();
        assert!(ms.total_bytes < mb.total_bytes);
    }

    #[test]
    fn dir_size_counts_files_recursively() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("a/x"), [0u8; 10]).unwrap();
        fs::write(dir.path().join("a/b/y"), [0u8; 5]).unwrap();
        assert_eq!(dir_size(dir.path()).unwrap(), 15);
    }

    #[test]
    fn text_blob_hits_target_and_varies() {
        let blob = text_blob(42, 0, 0, 4096);
        assert!(blob.len() >= 4096);
        let other = text_blob(42, 0, 1, 4096);
        assert_ne!(blob, other);
    }
}
