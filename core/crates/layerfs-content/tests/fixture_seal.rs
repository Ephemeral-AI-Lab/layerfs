//! The sealed Stage 5 fixtures, read only.
//!
//! The fixtures under `tests/fixtures/filesystem/` were written by a generator
//! that belongs to the reference implementation and is not part of core. It is
//! pinned, by blob and recovery commit, in `tests/fixtures/filesystem.generator`,
//! with its two contract properties: it is `#[ignore]`d, and it refuses to run
//! without `LAYERFS_SEAL_FIXTURES=1`. This file reads nothing outside `core/`
//! and never writes into the checkout. On every ordinary `cargo test` it
//!
//! - hashes the sealed directory and compares it with the frozen list in
//!   `tests/fixtures/filesystem.seal`, failing when a fixture is added, removed
//!   or changed;
//! - compares the generator record with the constants below, so the pin and
//!   the two contract strings cannot drift silently;
//! - reads every Rust source under `core/` and fails when one that could
//!   reseal (it names the gate, or names the sealed directory and writes
//!   files) lacks either contract string. No such source exists today.
//!
//! The last check is a text guard, as its predecessor was. A writer it cannot
//! recognize still turns the first check red on the next run.

mod support;

use std::path::{Path, PathBuf};

/// Directory holding the sealed fixtures.
fn seal_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filesystem")
}

/// The frozen digest list, one `blake3  name` pair per line.
fn seal_list() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filesystem.seal")
}

/// Parses the seal list, ignoring comments and blank lines.
fn expected_entries() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(seal_list()).expect("seal list");
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let (digest, name) = line.split_once("  ").expect("seal line");
            (name.to_owned(), digest.to_owned())
        })
        .collect()
}

/// Every fixture file present, in name order.
fn present_entries() -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = std::fs::read_dir(seal_directory())
        .expect("sealed fixture directory")
        .map(|entry| entry.expect("directory entry"))
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = std::fs::read(entry.path()).expect("fixture bytes");
            (name, blake3::hash(&bytes).to_hex().to_string())
        })
        .collect();
    entries.sort();
    entries
}

#[test]
fn the_sealed_fixtures_are_exactly_the_frozen_ones() {
    let expected = expected_entries();
    let present = present_entries();
    assert!(
        !expected.is_empty(),
        "the seal list is empty: it must name every sealed fixture"
    );
    let named = |entries: &[(String, String)]| {
        entries
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        named(&present),
        named(&expected),
        "the sealed directory no longer holds exactly the frozen files"
    );
    for ((name, digest), (_, want)) in present.iter().zip(expected.iter()) {
        assert_eq!(
            digest, want,
            "{name} changed: the seal moved without a deliberate reseal"
        );
    }
}

/// The generator's two contract properties, exactly as its source spells them.
const IGNORE_CONTRACT: &str = "#[ignore = \"rewrites the sealed fixtures in place";
const GATE_CONTRACT: &str = "LAYERFS_SEAL_FIXTURES";
/// The pinned generator: every key and value of the record, in order.
const GENERATOR: [(&str, &str); 8] = [
    (
        "path",
        "crates/layerfs-content/tests/stage5_reference_fixtures.rs",
    ),
    ("blob", "3a425d5a4318b0a9c7e7548f74be3edce5cedf4d"),
    (
        "sha256",
        "9dc8dd61a3de376417048403baacd8d9f59483c6ec6be4016dbaef59a46eec8d",
    ),
    (
        "recovery_commit",
        "f8a0a5ff1cd5f93b90bf16f964705b4c7d773615",
    ),
    ("recovery_tree", "498dd1917812ae90efb8841f57e22bfc284e96fb"),
    ("ignore_contract", IGNORE_CONTRACT),
    ("gate_contract", GATE_CONTRACT),
    (
        "reseal",
        "git archive of recovery_commit into an empty directory; there, LAYERFS_SEAL_FIXTURES=1 \
         and --ignored; diff against the committed directory; never in place",
    ),
];
/// How a source names the sealed directory, and the spelled calls that
/// create, replace or remove files. Bare words such as `symlink` are domain
/// terms here and are not calls.
const SEALED_DIRECTORY: &str = "fixtures/filesystem";
const WRITE_CALLS: [&str; 12] = [
    "fs::write",
    "File::create",
    ".create_new(",
    "fs::create_dir",
    "OpenOptions",
    "fs::copy",
    "fs::rename",
    "fs::remove_file",
    "fs::remove_dir",
    ".set_len(",
    "fs::hard_link",
    "fs::symlink",
];
/// Directories that hold no first-party source.
const SKIPPED: [&str; 4] = ["target", "vendor", "docs", ".git"];

/// What differs between a generator record and the pinned constants.
fn record_differences(text: &str) -> Vec<String> {
    let found: Vec<(&str, &str)> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split_once("  ").unwrap_or((line, "")))
        .collect();
    let mut differences = Vec::new();
    for index in 0..found.len().max(GENERATOR.len()) {
        if found.get(index) != GENERATOR.get(index) {
            differences.push(format!(
                "entry {index}: record {:?}, pinned {:?}",
                found.get(index),
                GENERATOR.get(index)
            ));
        }
    }
    differences
}

/// Every Rust source beneath `root`, in path order.
fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("source directory") {
            let entry = entry.expect("directory entry");
            let kind = entry.file_type().expect("entry type");
            let path = entry.path();
            if kind.is_dir() {
                if !SKIPPED.iter().any(|name| entry.file_name() == *name) {
                    pending.push(path);
                }
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}

/// Sources beneath `root` that could reseal and lack a contract string. Only
/// the file at exactly `guard` is exempt.
fn ungated_writers(root: &Path, guard: &Path) -> Vec<PathBuf> {
    rust_sources(root)
        .into_iter()
        .filter(|path| path != guard)
        .filter(|path| {
            let text =
                String::from_utf8_lossy(&std::fs::read(path).expect("source bytes")).into_owned();
            let writes = WRITE_CALLS.iter().any(|call| text.contains(call));
            let could_reseal =
                text.contains(GATE_CONTRACT) || (text.contains(SEALED_DIRECTORY) && writes);
            could_reseal && !(text.contains(IGNORE_CONTRACT) && text.contains(GATE_CONTRACT))
        })
        .collect()
}

/// A scratch source tree outside the checkout, removed when dropped.
struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "layerfs-fixture-seal-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }
    fn source(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("scratch parent");
        std::fs::write(&path, text).expect("scratch source");
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const WRITER: &str =
    "fn reseal() { std::fs::write(\"tests/fixtures/filesystem/a.bin\", b\"\").unwrap(); }\n";
const IGNORED: &str = "#[ignore = \"rewrites the sealed fixtures in place; see the guard\"]\n";
const GATED: &str =
    "fn gate() { assert_eq!(std::env::var(\"LAYERFS_SEAL_FIXTURES\").as_deref(), Ok(\"1\")); }\n";

#[test]
fn the_generator_record_is_the_pinned_one() {
    let record = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filesystem.generator");
    let text = std::fs::read_to_string(record).expect("generator record");
    assert_eq!(
        record_differences(&text),
        Vec::<String>::new(),
        "the generator record moved away from the pinned generator contract"
    );
}

#[test]
fn no_source_in_core_can_reseal_during_an_ordinary_test_run() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let core = manifest
        .parent()
        .and_then(Path::parent)
        .expect("core directory");
    assert!(
        core.join("Cargo.toml").is_file() && core.join("crates").is_dir(),
        "the scan root is not the core workspace"
    );
    let sources = rust_sources(core);
    let guard = manifest.join("tests/fixture_seal.rs");
    assert!(
        sources.contains(&guard),
        "the scan did not reach this guard's own source"
    );
    assert_eq!(
        ungated_writers(core, &guard),
        Vec::<PathBuf>::new(),
        "a source could rewrite the sealed fixtures without the ignore and the gate"
    );
}

#[test]
fn an_altered_generator_record_is_reported() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filesystem.generator"),
    )
    .expect("generator record");
    for (from, to) in [
        (GATE_CONTRACT, "LAYERFS_RESEAL"),
        ("#[ignore = ", "#[test = "),
        ("3a425d5a4318b0a9c7e7548f74be3edce5cedf4d", "0"),
        ("recovery_commit  ", "recovery_commit "),
    ] {
        assert!(text.contains(from), "the record lacks {from:?}");
        assert!(
            !record_differences(&text.replacen(from, to, 1)).is_empty(),
            "replacing {from:?} went unreported"
        );
    }
    let without_gate = text
        .lines()
        .filter(|line| !line.starts_with("gate_contract"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!record_differences(&without_gate).is_empty());
    assert!(!record_differences(&format!("{text}extra  value\n")).is_empty());
}

#[test]
fn a_writer_without_the_ignore_or_the_gate_is_reported() {
    let scratch = Scratch::new("ungated");
    let guard = scratch.source("crates/a/tests/fixture_seal.rs", WRITER);
    let bare = scratch.source("crates/a/tests/writer.rs", WRITER);
    let ignored_only = scratch.source("crates/b/tests/writer.rs", &format!("{IGNORED}{WRITER}"));
    let gated_only = scratch.source("crates/c/examples/writer.rs", &format!("{GATED}{WRITER}"));
    let gate_named = scratch.source("benchmark/d/src/main.rs", GATED);
    let same_name = scratch.source("crates/e/tests/fixture_seal.rs", WRITER);
    scratch.source("crates/a/target/debug/writer.rs", WRITER);
    scratch.source("crates/a/tests/writer.txt", WRITER);
    let mut expected = vec![bare, ignored_only, gated_only, gate_named, same_name];
    expected.sort();
    assert_eq!(ungated_writers(&scratch.0, &guard), expected);
}

#[test]
fn a_gated_ignored_writer_and_plain_readers_are_not_reported() {
    let scratch = Scratch::new("gated");
    let guard = scratch.source("crates/a/tests/fixture_seal.rs", WRITER);
    scratch.source(
        "crates/a/tests/generator.rs",
        &format!("{IGNORED}{GATED}{WRITER}"),
    );
    scratch.source(
        "crates/a/tests/reader.rs",
        "const BYTES: &[u8] = include_bytes!(\"fixtures/filesystem/a.bin\");\n",
    );
    scratch.source(
        "crates/a/tests/other_writer.rs",
        "fn scratch() { std::fs::write(\"/tmp/unrelated\", b\"\").unwrap(); }\n",
    );
    assert_eq!(ungated_writers(&scratch.0, &guard), Vec::<PathBuf>::new());
}
