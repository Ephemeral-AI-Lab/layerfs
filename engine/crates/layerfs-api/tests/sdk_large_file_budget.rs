//! A Store created with a larger construction cutoff (`LAYERFS_STORE_POLICY`
//! `smallFileThreshold`, up to 1 MiB) keeps whole files above the default size,
//! and its payload chain budgets scale with that cutoff (#472). The second
//! version of such a file is saved, as a delta against the first, through
//! `fs.apply`, `fs.write` and a Workspace Commit; Project Init of two similar
//! large files succeeds. Under the fixed 512 KiB / 256 KiB budgets these Saves
//! failed with a dependency-work `Integrity` error, and after #473 stored every
//! second version whole.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_storage::StoragePolicy;
use std::fs;
use support::{write, Change, Fixture};

fn mebibyte_cutoff() -> StoragePolicy {
    StoragePolicy::new(1, 1_048_576, 16, 8).validated().unwrap()
}

/// CSV text of `len` bytes, one record per line: it compresses to about half,
/// like a data export or log.
fn rows(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut out = Vec::with_capacity(len + 64);
    let mut row = 0_u64;
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        row += 1;
        out.extend_from_slice(
            format!(
                "{row},2024-{:02}-{:02},{},{:016x}\n",
                state % 12 + 1,
                state % 28 + 1,
                state % 1_000_000,
                state
            )
            .as_bytes(),
        );
    }
    out.truncate(len);
    out
}

/// Word-salad text of `len` bytes from eight words. Init's similarity index
/// pairs two such files; the codec finds the nearby repeats first, so a delta
/// need not beat FULL, but the Save must not fail.
fn words(len: usize) -> Vec<u8> {
    const WORDS: [&str; 8] = [
        "alpha", "beta", "gamma", "delta", "layer", "branch", "commit", "store",
    ];
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(WORDS[(state % 8) as usize].as_bytes());
        out.push(if state % 9 == 0 { b'\n' } else { b' ' });
    }
    out.truncate(len);
    out
}

/// Pseudo-random bytes: they are stored as they are.
fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// The same bytes with ten of them overwritten in the middle.
fn edited(bytes: &[u8]) -> Vec<u8> {
    let mut second = bytes.to_vec();
    let middle = second.len() / 2;
    second[middle..middle + 10].fill(b'#');
    second
}

/// An installed Store under `policy` with a Branch `w` that holds `/big` as `first`.
fn with_first_version(label: &str, policy: StoragePolicy, first: &[u8]) -> Fixture {
    let host = installed::Fixture::built_with(label, policy, |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    let fixture = Fixture::open(host);
    fixture.fork("w");
    fixture.apply("w", vec![write("/big", first)]);
    fixture
}

/// Whole-file objects this Store's Saves stored at a positive depth so far.
fn whole_file_deltas(fixture: &Fixture) -> u64 {
    let depths = fixture
        .service
        .store
        .save_counters()
        .delta
        .whole_file_depths;
    depths[1..].iter().sum()
}

fn assert_size(fixture: &Fixture, path: &str, size: usize) {
    let stat = fixture.call(
        "fs.stat",
        vec![
            ("branch", SdkValue::text("w")),
            ("path", SdkValue::text(path)),
        ],
    );
    assert_eq!(
        stat.get("size"),
        Some(&SdkValue::Int(size as i64)),
        "{path}"
    );
}

/// Branch `w` holds `expected`'s size at `path`, and its line that holds `offset`.
fn assert_text_at(fixture: &Fixture, path: &str, expected: &[u8], offset: usize) {
    assert_size(fixture, path, expected.len());
    let number = expected[..offset].iter().filter(|b| **b == b'\n').count();
    let line = expected.split(|b| *b == b'\n').nth(number).unwrap();
    let reply = fixture.call(
        "fs.read",
        vec![
            ("branch", SdkValue::text("w")),
            ("path", SdkValue::text(path)),
            ("startLine", SdkValue::Int(number as i64 + 1)),
            ("endLine", SdkValue::Int(number as i64 + 1)),
        ],
    );
    assert_eq!(
        reply.get_text("text").map(str::trim_end),
        Some(std::str::from_utf8(line).unwrap()),
        "{path}"
    );
}

#[test]
fn fs_apply_stores_a_second_large_text_version_as_a_delta() {
    // 600,000 bytes of text: a canonical object above the fixed 512 KiB.
    let first = rows(600_000);
    let second = edited(&first);
    let fixture = with_first_version("budget-apply", mebibyte_cutoff(), &first);
    let before = whole_file_deltas(&fixture);
    fixture.apply("w", vec![write("/big", second.as_slice())]);
    assert_eq!(
        whole_file_deltas(&fixture) - before,
        1,
        "{:?}",
        fixture.service.store.save_counters().delta
    );
    assert_text_at(&fixture, "/big", &second, second.len() / 2);
    fixture.cleanup();
}

#[test]
fn fs_write_stores_a_second_large_incompressible_version_as_a_delta() {
    // 300,000 incompressible bytes: a record above the fixed 256 KiB encoded.
    let first = noise(300_000);
    let second = edited(&first);
    let fixture = with_first_version("budget-write", mebibyte_cutoff(), &first);
    let before = whole_file_deltas(&fixture);
    let reply = fixture.call(
        "fs.write",
        vec![
            ("branch", SdkValue::text("w")),
            ("path", SdkValue::text("/big")),
            ("bytes", SdkValue::Bytes(second.clone())),
        ],
    );
    assert_eq!(reply.get_text("kind"), Some("Committed"), "{reply:?}");
    assert_eq!(
        whole_file_deltas(&fixture) - before,
        1,
        "{:?}",
        fixture.service.store.save_counters().delta
    );
    assert_size(&fixture, "/big", second.len());
    fixture.cleanup();
}

#[test]
fn a_workspace_commit_stores_a_second_large_text_version_as_a_delta() {
    // A private Workspace overwrites the first ten bytes and its captured
    // producer builds the new version; no FUSE mount is involved.
    let first = rows(600_000);
    let mut second = first.clone();
    second[..10].fill(b'#');
    let fixture = with_first_version("budget-commit", mebibyte_cutoff(), &first);
    let before = whole_file_deltas(&fixture);
    fixture.workspace_commit("w", &[Change::Write("/big", &second[..10])], 1, || {});
    assert_eq!(
        whole_file_deltas(&fixture) - before,
        1,
        "{:?}",
        fixture.service.store.save_counters().delta
    );
    assert_text_at(&fixture, "/big", &second, 0);
    fixture.cleanup();
}

#[test]
fn init_of_two_similar_large_text_files_stores_both() {
    let first = words(600_000);
    let second = edited(&first);
    let host = installed::Fixture::built_with("budget-init", mebibyte_cutoff(), |source| {
        fs::write(source.join("data-2024.csv"), &first).unwrap();
        fs::write(source.join("data-2025.csv"), &second).unwrap();
        2
    });
    let fixture = Fixture::open(host);
    fixture.fork("w");
    assert_text_at(&fixture, "/data-2024.csv", &first, first.len() / 2);
    assert_text_at(&fixture, "/data-2025.csv", &second, second.len() / 2);
    fixture.cleanup();
}

#[test]
fn control_the_default_cutoff_saves_the_same_two_versions() {
    // At the default cutoff the 600,000-byte file is chunked.
    let first = words(600_000);
    let second = edited(&first);
    let fixture = with_first_version("budget-default", StoragePolicy::frozen_default(), &first);
    fixture.apply("w", vec![write("/big", second.as_slice())]);
    assert_size(&fixture, "/big", second.len());
    fixture.cleanup();
}
