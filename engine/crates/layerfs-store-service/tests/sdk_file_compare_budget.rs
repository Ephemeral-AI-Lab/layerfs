//! Different-root equality is exact or refused before payload work, never guessed.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_content::compare::EntryFacts;
use layerfs_store_service::read::{file_facts, snapshot};
use std::fs;
use support::{write, Change, Fixture};

fn entry(f: &Fixture, at: &str, path: &str) -> EntryFacts {
    let stack =
        layerfs_store_service::history::lookup::project(&f.service.store, "installed").unwrap();
    let (reader, snap) = snapshot::open(&f.service.store, &stack, at).unwrap();
    snapshot::existing(reader.as_ref(), &snap, path).unwrap()
}
fn selector(at: &str, path: &str) -> SdkValue {
    SdkValue::map([
        ("commit", SdkValue::text(at)),
        ("path", SdkValue::text(path)),
    ])
}
fn diff(f: &Fixture, extended: bool) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    let mut args = vec![
        ("left", selector("main", "/large")),
        ("right", selector("work", "/large")),
    ];
    if extended {
        args.push(("page", SdkValue::Int(1)));
    }
    f.try_call("fs.diff_files", args)
}

#[test]
fn new_queries_reserve_large_different_root_payload_but_keep_same_root_and_legacy_reads() {
    let f = Fixture::new("file-comparison-admission", |source| {
        fs::write(source.join("large"), vec![b'a'; 33 * 1024 * 1024]).unwrap();
        1
    });
    f.fork("work");
    let same = diff(&f, true).unwrap();
    assert!(same.get_bool("contentEqual"));
    let original = entry(&f, "main", "/large");
    f.workspace_commit("work", &[Change::Write("/large", b"x")], 209, || {});
    let changed = entry(&f, "work", "/large");
    assert_ne!(original.value.content_root, changed.value.content_root);
    assert_eq!(diff(&f, true).unwrap_err().code, "Capacity");
    let revisions = f
        .try_call(
            "fs.revision",
            vec![
                ("branch", SdkValue::text("work")),
                ("path", SdkValue::text("/large")),
                ("page", SdkValue::Int(1)),
                ("pageSize", SdkValue::Int(1)),
            ],
        )
        .unwrap_err();
    assert_eq!(revisions.code, "Capacity");
    let legacy = diff(&f, false).unwrap();
    assert!(!legacy.get_bool("contentEqual"));
    assert_eq!(legacy.get_text("kind"), Some("Binary"));
    f.cleanup();
}

#[test]
fn equal_bytes_under_different_roots_are_unknown_when_the_request_cannot_admit_them() {
    let mut state = 7u64;
    let base = (0..400 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect::<Vec<_>>();
    let f = Fixture::new("file-comparison-equal-root-budget", |source| {
        fs::write(source.join("file"), &base).unwrap();
        1
    });
    f.fork("work");
    let edited = |bytes: &[u8], at: usize, byte: u8| {
        let mut out = bytes[..at].to_vec();
        out.extend(vec![byte; 6000]);
        out.extend_from_slice(&bytes[at + 3000..]);
        out
    };
    let original = edited(&base, 40_000, 8);
    let start = f.apply("work", vec![write("/file", original.clone())]);
    let start_id = start.get_text("head").unwrap().to_owned();
    f.apply("work", vec![write("/file", edited(&original, 300_000, 9))]);
    f.apply("work", vec![write("/file", original)]);
    let a = entry(&f, &start_id, "/file");
    let b = entry(&f, "work", "/file");
    assert_ne!(
        a.value.content_root, b.value.content_root,
        "fixture must exercise different roots with equal bytes"
    );
    let stack =
        layerfs_store_service::history::lookup::project(&f.service.store, "installed").unwrap();
    let reader = snapshot::reader(&f.service.store, &stack);
    let mut none = 0;
    assert_eq!(
        file_facts::changes_bounded(&reader, Some(a), Some(b), &mut none)
            .unwrap_err()
            .code,
        "Capacity"
    );
    assert_eq!(none, 0);
    let mut enough = file_facts::CONTENT_QUERY_BYTES;
    let (content, _, _) =
        file_facts::changes_bounded(&reader, Some(a), Some(b), &mut enough).unwrap();
    assert!(
        !content,
        "over-budget work must never become a false content change"
    );
    assert!(enough < file_facts::CONTENT_QUERY_BYTES);
    let mut exactly_one_pair = file_facts::CONTENT_QUERY_BYTES - enough;
    assert!(
        !file_facts::changes_bounded(&reader, Some(a), Some(b), &mut exactly_one_pair)
            .unwrap()
            .0
    );
    assert_eq!(exactly_one_pair, 0);
    assert_eq!(
        file_facts::changes_bounded(&reader, Some(a), Some(b), &mut exactly_one_pair)
            .unwrap_err()
            .code,
        "Capacity"
    );
    let refused = file_facts::changes_bounded(&reader, Some(a), Some(b), &mut 1).unwrap_err();
    assert_eq!(refused.code, "Capacity");
    f.cleanup();
}

#[test]
fn equal_roots_and_known_different_sizes_do_not_spend_payload_budget() {
    let f = Fixture::new("file-comparison-budget-fast-facts", |source| {
        fs::write(source.join("short"), b"a").unwrap();
        fs::write(source.join("long"), b"ab").unwrap();
        2
    });
    let a = entry(&f, "main", "/short");
    let b = entry(&f, "main", "/long");
    let stack =
        layerfs_store_service::history::lookup::project(&f.service.store, "installed").unwrap();
    let reader = snapshot::reader(&f.service.store, &stack);
    let mut none = 0;
    assert!(
        !file_facts::changes_bounded(&reader, Some(a), Some(a), &mut none)
            .unwrap()
            .0
    );
    assert!(
        file_facts::changes_bounded(&reader, Some(a), Some(b), &mut none)
            .unwrap()
            .0
    );
    assert_eq!(none, 0);
    f.cleanup();
}
