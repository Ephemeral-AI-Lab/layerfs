//! A file written to other bytes and back to its own is no content change,
//! though its root may differ: a root depends on how the file was built
//! (#352 F-073, range 058: `diff_commits`, now `commit.diff`, listed a reconciled file whose bytes
//! were the same at both Commits).
use crate::support;

use layerfs_history::CommitOutcome;
use layerfs_protocol::sdk::SdkValue;
use std::fs;
use support::{items, write, Change, Fixture};

const PATH: &str = "/src/catalog.bin";
/// Chunked: above the 128 KiB whole-file cutoff.
const LENGTH: usize = 400 * 1024;

fn noise(seed: u64, length: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// `base` with the 3,000 bytes at `at` replaced by 6,000 others.
fn edited(base: &[u8], at: usize, seed: u64) -> Vec<u8> {
    let mut out = base[..at].to_vec();
    out.extend(noise(seed, 6_000));
    out.extend_from_slice(&base[at + 3_000..]);
    out
}

/// The Branch `work` at `start`, where the file holds `bytes`, built by an
/// edit of the bytes Init saved, as range 057's reconcile built it.
struct Start {
    fixture: Fixture,
    start: String,
    bytes: Vec<u8>,
}

fn fixture(label: &str) -> Start {
    let original = noise(7, LENGTH);
    let bytes = original.clone();
    let fixture = Fixture::new(label, move |source| {
        fs::create_dir(source.join("src")).unwrap();
        fs::write(source.join("src/catalog.bin"), &bytes).unwrap();
        fs::write(source.join("other.txt"), b"0\n").unwrap();
        3
    });
    fixture.fork("work");
    let bytes = edited(&original, 40_000, 8);
    let start = head(&fixture.apply("work", vec![write(PATH, bytes.clone())]));
    Start {
        fixture,
        start,
        bytes,
    }
}

fn head(reply: &SdkValue) -> String {
    reply.get_text("head").unwrap().to_owned()
}

/// The `PATH` item `commit.diff` lists from `from` to `to`, if any.
fn item(fixture: &Fixture, from: &str, to: &str) -> Option<SdkValue> {
    let diff = fixture.call(
        "commit.diff",
        vec![("ref", SdkValue::text(to)), ("from", SdkValue::text(from))],
    );
    items(&diff)
        .iter()
        .find(|item| item.get_text("path") == Some(PATH))
        .cloned()
}

/// Whether `PATH` holds the same bytes at `from` and `to`: `commit.diff`
/// lists no content change for it.
fn content_equal(fixture: &Fixture, from: &str, to: &str) -> bool {
    item(fixture, from, to).is_none_or(|item| !item.get_bool("content"))
}

fn committed(fixture: &Fixture, changes: &[Change<'_>], id: u8) -> String {
    let done = fixture.workspace_commit("work", changes, id, || {});
    let Some(CommitOutcome::Committed { record, .. }) = done.history else {
        panic!("Workspace Commit: {:?}", done.history);
    };
    record.id.to_string()
}

#[test]
fn a_file_written_away_and_back_through_the_store_is_no_content_change() {
    let Start {
        fixture,
        start,
        bytes: original,
    } = fixture("sdk-written-back-store");
    let away = head(&fixture.apply("work", vec![write(PATH, edited(&original, 300_000, 9))]));
    let back = head(&fixture.apply("work", vec![write(PATH, original.clone())]));

    let changed = item(&fixture, &start, &away).expect("the edit is listed");
    assert!(changed.get_bool("content"), "{changed:?}");
    if let Some(listed) = item(&fixture, &start, &back) {
        assert_eq!(listed.get_text("change"), Some("modified"), "{listed:?}");
        assert!(!listed.get_bool("content"), "{listed:?}");
    }

    // One byte at the end, the same length: still a content change.
    let mut last = original.clone();
    *last.last_mut().unwrap() ^= 1;
    let flipped = head(&fixture.apply("work", vec![write(PATH, last)]));
    let listed = item(&fixture, &back, &flipped).expect("the flipped byte is listed");
    assert!(listed.get_bool("content"), "{listed:?}");
    assert!(!content_equal(&fixture, &back, &flipped));
    fixture.cleanup();
}

#[test]
fn a_file_saved_away_and_back_by_rename_over_is_no_content_change() {
    let Start {
        fixture,
        start,
        bytes: original,
    } = fixture("sdk-written-back-rename");
    let away = committed(
        &fixture,
        &[Change::SaveOver(PATH, &edited(&original, 300_000, 9))],
        41,
    );
    let back = committed(&fixture, &[Change::SaveOver(PATH, &original)], 42);

    let changed = item(&fixture, &start, &away).expect("the edit is listed");
    assert!(changed.get_bool("content"), "{changed:?}");
    // A rename gives the path a new inode: listed, but not as a content change.
    let listed = item(&fixture, &start, &back).expect("the new inode is listed");
    assert!(listed.get_bool("identity"), "{listed:?}");
    assert!(!listed.get_bool("content"), "{listed:?}");
    assert!(content_equal(&fixture, &start, &back));
    fixture.cleanup();
}

#[test]
fn a_merge_of_two_sides_that_wrote_the_same_bytes_does_not_conflict() {
    let Start {
        fixture,
        bytes: original,
        ..
    } = fixture("sdk-written-back-merge");
    for side in ["ours", "theirs"] {
        fixture.call(
            "branch.fork",
            vec![
                ("name", SdkValue::text(side)),
                ("from", SdkValue::text("work")),
            ],
        );
    }
    // Both sides end at the same bytes, built over different predecessors:
    // the same bytes under two roots. Not text, so a false difference would
    // be a binary conflict.
    let same = edited(&original, 300_000, 9);
    let ours = head(&fixture.apply("ours", vec![write(PATH, same.clone())]));
    fixture.apply("theirs", vec![write(PATH, edited(&original, 100_000, 10))]);
    let theirs = head(&fixture.apply("theirs", vec![write(PATH, same)]));
    assert!(content_equal(&fixture, &ours, &theirs));

    let merged = fixture
        .try_call(
            "branch.merge",
            vec![
                ("branch", SdkValue::text("ours")),
                ("with", SdkValue::text("theirs")),
                ("name", SdkValue::text("merged")),
            ],
        )
        .unwrap_or_else(|refusal| panic!("equal sides conflicted: {refusal:?}"));
    assert_eq!(merged.get_text("kind"), Some("Merged"), "{merged:?}");
    assert!(content_equal(&fixture, &ours, "merged"));
    fixture.cleanup();
}
