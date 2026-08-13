//! Shared fixtures for integration tests (docs/05 §6: generated
//! programmatically; the repo-root `fixtures/` dir is for static files that
//! can't be). One canonical source tree with nested dirs, an empty dir and
//! Arabic/Unicode names, plus a byte-exact tree comparator.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const ARABIC_DIR: &str = "مجلد عربي";
pub const ARABIC_FILE: &str = "ملف.txt";
pub const ARABIC_CONTENT: &[u8] = "محتوى عربي".as_bytes();

/// `<root>/data/…` — a single-root tree (extracts as-is per docs/03 F3).
pub fn build_source_tree(root: &Path) {
    let data = root.join("data");
    fs::create_dir_all(data.join("nested")).unwrap();
    fs::create_dir_all(data.join(ARABIC_DIR)).unwrap();
    fs::create_dir_all(data.join("empty")).unwrap();
    fs::write(data.join("hello.txt"), b"hello world").unwrap();
    fs::write(
        data.join("nested").join("deep.bin"),
        (0u16..=255).map(|b| b as u8).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(data.join(ARABIC_DIR).join(ARABIC_FILE), ARABIC_CONTENT).unwrap();
}

#[derive(Debug, PartialEq, Eq)]
pub enum Entry {
    Dir,
    File(Vec<u8>),
}

/// Byte-compare two directory trees (relative paths + kinds + contents).
pub fn assert_trees_equal(a: &Path, b: &Path) {
    let a_entries = collect(a);
    let b_entries = collect(b);
    assert_eq!(a_entries, b_entries, "tree mismatch: {a:?} vs {b:?}");
}

fn collect(root: &Path) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    for item in walkdir::WalkDir::new(root).follow_links(false) {
        let item = item.unwrap();
        let rel = item
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() {
            continue;
        }
        let entry = if item.file_type().is_dir() {
            Entry::Dir
        } else {
            Entry::File(fs::read(item.path()).unwrap())
        };
        out.insert(rel, entry);
    }
    out
}
