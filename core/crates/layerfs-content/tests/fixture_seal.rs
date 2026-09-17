//! The sealed Stage 5 fixtures, read only.
//!
//! `crates/layerfs-content/tests/stage5_reference_fixtures.rs` is the generator:
//! it rewrites `tests/fixtures/filesystem/` in place and is now `#[ignore]`d and
//! refuses to run without `LAYERFS_SEAL_FIXTURES=1`. This file is its read-only
//! counterpart and runs on every ordinary `cargo test`: it hashes the sealed
//! directory and compares it with the frozen list in `tests/fixtures/filesystem.seal`.
//! It never writes anything, and it fails when a fixture is added, removed or
//! changed - so an accidental reseal can no longer move the seal the Stage 5
//! comparison is measured against without a red test.

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

#[test]
fn the_generator_cannot_reseal_during_an_ordinary_test_run() {
    // The generator's own source is the contract: it must stay `#[ignore]`d and
    // must keep refusing to run without the explicit environment variable.
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../crates/layerfs-content/tests/stage5_reference_fixtures.rs"),
    )
    .expect("generator source");
    assert!(
        source.contains("#[ignore = \"rewrites the sealed fixtures in place"),
        "the generator is no longer ignored: an ordinary `cargo test` could reseal"
    );
    assert!(
        source.contains("LAYERFS_SEAL_FIXTURES"),
        "the generator no longer gates its write behind the environment variable"
    );
}
