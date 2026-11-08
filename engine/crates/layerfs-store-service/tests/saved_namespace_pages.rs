//! Immutable numbered namespace pages exercise real indexed/resumed reads.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, Fixture};
fn text(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn page(f: &Fixture, method: &str, number: i64, size: i64, pattern: Option<&str>) -> SdkValue {
    let mut args = vec![
        ("branch", text("main")),
        ("path", text("/")),
        ("page", SdkValue::Int(number)),
        ("pageSize", SdkValue::Int(size)),
    ];
    if let Some(pattern) = pattern {
        args.push(("pattern", text(pattern)));
    }
    f.call(method, args)
}
#[test]
fn deep_empty_directory_and_filtered_queries_prove_exhaustion_without_ordinal_caps() {
    let f = Fixture::new("namespace-empty", |_| 0);
    for (method, pattern) in [
        ("fs.list", None),
        ("fs.glob", Some("*.missing")),
        ("fs.grep", Some("needle")),
    ] {
        let result = page(&f, method, 10001, 50, pattern);
        assert!(items(&result).is_empty());
        assert!(!result.get_bool("hasMore"));
    }
    f.cleanup();
}
#[test]
fn sequential_list_and_glob_pages_cross_the_replay_budget_using_indexed_boundaries() {
    let f = Fixture::new("namespace-deep-pages", |source| {
        for n in 0..10020 {
            fs::write(source.join(format!("f{n:05}")), b"").unwrap();
        }
        10020
    });
    for (method, pattern) in [("fs.list", None), ("fs.glob", Some("/f*"))] {
        let mut count = 0;
        for number in 1..=201 {
            let result = page(&f, method, number, 50, pattern);
            count += items(&result).len();
            assert_eq!(
                result.get_bool("hasMore"),
                number < 201,
                "{method} page {number}"
            );
        }
        assert_eq!(count, 10020);
    }
    f.cleanup();
}
#[test]
fn grep_resumes_within_a_file_and_preserves_binary_skip_counts_on_cold_and_cached_pages() {
    let f = Fixture::new("namespace-grep-resume", |source| {
        fs::write(source.join("a"), [0u8, 255]).unwrap();
        fs::write(
            source.join("b"),
            "needle one\nno\nneedle two\nneedle three\n",
        )
        .unwrap();
        fs::write(source.join("c"), [0u8, 255]).unwrap();
        fs::write(source.join("d"), "needle four\n").unwrap();
        4
    });
    let cold = page(&f, "fs.grep", 2, 1, Some("needle"));
    let first = page(&f, "fs.grep", 1, 1, Some("needle"));
    assert_eq!(items(&first)[0].get_int("line"), Some(1));
    let warm = page(&f, "fs.grep", 2, 1, Some("needle"));
    assert_eq!(warm, cold);
    assert_eq!(items(&warm)[0].get_int("line"), Some(3));
    let third = page(&f, "fs.grep", 3, 1, Some("needle"));
    assert_eq!(items(&third)[0].get_int("line"), Some(4));
    let fourth = page(&f, "fs.grep", 4, 1, Some("needle"));
    assert_eq!(items(&fourth)[0].get_text("path"), Some("/d"));
    assert_eq!(fourth.get_int("skipped"), Some(2));
    let cached_end = page(&f, "fs.grep", 5, 1, Some("needle"));
    let cold_end = page(&f, "fs.grep", 6, 1, Some("needle"));
    assert!(items(&cached_end).is_empty());
    assert!(items(&cold_end).is_empty());
    assert_eq!(cached_end.get_int("skipped"), Some(2));
    assert_eq!(cold_end.get_int("skipped"), Some(2));
    f.cleanup();
}
#[test]
fn grep_admits_delivered_prefixes_of_large_text_and_binary_files() {
    let f = Fixture::new("namespace-grep-large-prefix", |source| {
        let mut data = vec![b'x'; 17 * 1024 * 1024];
        data[0] = 0;
        fs::write(source.join("a"), &data).unwrap();
        data[..14].copy_from_slice(b"needle\nneedle\n");
        fs::write(source.join("b"), &data).unwrap();
        2
    });
    let result = page(&f, "fs.grep", 1, 1, Some("needle"));
    assert_eq!(items(&result).len(), 1);
    assert_eq!(items(&result)[0].get_text("path"), Some("/b"));
    assert!(result.get_bool("hasMore"));
    assert_eq!(result.get_int("skipped"), Some(1));
    f.cleanup();
}
