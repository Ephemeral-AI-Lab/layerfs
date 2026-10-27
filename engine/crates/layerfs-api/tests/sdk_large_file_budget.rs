//! A Store created with a larger construction cutoff (`LAYERFS_STORE_POLICY`
//! `smallFileThreshold`, 128 KiB to 1 MiB) holds whole files above the default
//! size, and a second version of such a file is saved even when the first is
//! larger than the delta chain budgets (512 KiB canonical, 256 KiB encoded): the
//! new version is stored independently.
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

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// Word-salad text of `len` bytes: it compresses like source code or logs.
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

/// The line of `bytes` holding `offset`: its 1-based number and its text.
fn line_at(bytes: &[u8], offset: usize) -> (i64, String) {
    let number = bytes[..offset].iter().filter(|b| **b == b'\n').count();
    let line = bytes.split(|b| *b == b'\n').nth(number).unwrap();
    (number as i64 + 1, String::from_utf8(line.to_vec()).unwrap())
}

/// Branch `w` holds `expected` at the line that holds `offset`.
fn assert_line_at(fixture: &Fixture, expected: &[u8], offset: usize) {
    let (number, line) = line_at(expected, offset);
    let reply = fixture.call(
        "fs.read",
        vec![
            ("branch", text("w")),
            ("path", text("/big")),
            ("startLine", SdkValue::Int(number)),
            ("endLine", SdkValue::Int(number)),
        ],
    );
    assert_eq!(
        reply.get_text("text").map(str::trim_end),
        Some(line.as_str())
    );
}

fn assert_size(fixture: &Fixture, path: &str, size: usize) {
    let stat = fixture.call("fs.stat", vec![("branch", text("w")), ("path", text(path))]);
    assert_eq!(
        stat.get("size"),
        Some(&SdkValue::Int(size as i64)),
        "{path}"
    );
}

#[test]
fn fs_apply_saves_a_second_version_of_a_large_text_file() {
    // 600,000 compressible bytes: a canonical object above the 512 KiB budget.
    let first = words(600_000);
    let second = edited(&first);
    let fixture = with_first_version("budget-apply", mebibyte_cutoff(), &first);
    fixture.apply("w", vec![write("/big", second.as_slice())]);
    assert_size(&fixture, "/big", second.len());
    assert_line_at(&fixture, &second, second.len() / 2);
    fixture.cleanup();
}

#[test]
fn fs_write_saves_a_second_version_of_a_large_incompressible_file() {
    // 300,000 incompressible bytes: a record above the 256 KiB encoded budget.
    let first = noise(300_000);
    let second = edited(&first);
    let fixture = with_first_version("budget-write", mebibyte_cutoff(), &first);
    let reply = fixture.call(
        "fs.write",
        vec![
            ("branch", text("w")),
            ("path", text("/big")),
            ("bytes", SdkValue::Bytes(second.clone())),
        ],
    );
    assert_eq!(reply.get_text("kind"), Some("Committed"), "{reply:?}");
    assert_size(&fixture, "/big", second.len());
    fixture.cleanup();
}

#[test]
fn a_workspace_commit_saves_a_second_version_of_a_large_text_file() {
    // A Workspace overwrites the first ten bytes and its captured producer builds
    // the new version through the same Content and Storage code as `fs.apply`;
    // no FUSE mount is involved.
    let first = words(600_000);
    let mut second = first.clone();
    second[..10].fill(b'#');
    let fixture = with_first_version("budget-commit", mebibyte_cutoff(), &first);
    fixture.workspace_commit("w", &[Change::Write("/big", &second[..10])], 1, || {});
    assert_size(&fixture, "/big", second.len());
    assert_line_at(&fixture, &second, 0);
    fixture.cleanup();
}

#[test]
fn init_of_a_directory_with_two_similar_large_text_files_stores_both() {
    // Project Init of a directory that contains two similar files.
    let first = words(600_000);
    let second = edited(&first);
    let host = installed::Fixture::built_with("budget-init", mebibyte_cutoff(), |source| {
        fs::write(source.join("data-2024.csv"), &first).unwrap();
        fs::write(source.join("data-2025.csv"), &second).unwrap();
        2
    });
    let fixture = Fixture::open(host);
    fixture.fork("w");
    assert_size(&fixture, "/data-2024.csv", first.len());
    assert_size(&fixture, "/data-2025.csv", second.len());
    fixture.cleanup();
}

#[test]
fn control_the_default_cutoff_saves_the_same_two_versions() {
    let first = words(600_000);
    let second = edited(&first);
    let fixture = with_first_version("budget-default", StoragePolicy::frozen_default(), &first);
    fixture.apply("w", vec![write("/big", second.as_slice())]);
    assert_size(&fixture, "/big", second.len());
    fixture.cleanup();
}
