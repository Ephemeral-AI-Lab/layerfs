//! SDK diff correctness and Store-demand regressions for wide immutable namespaces.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, operation, paths, write, Fixture};

#[test]
fn wide_diff_pages_have_bounded_store_demand() {
    let mut observed = Vec::new();
    for count in [1024usize, 2048] {
        let fixture = Fixture::new(&format!("sdk-diff-wide-{count}"), |source| {
            fs::create_dir(source.join("cache")).unwrap();
            fs::create_dir(source.join("work")).unwrap();
            for n in 0..count {
                fs::write(source.join(format!("cache/item-{n:06}")), b"untouched\n").unwrap();
            }
            fs::write(source.join("work/a"), b"before a\n").unwrap();
            fs::write(source.join("work/z"), b"before z\n").unwrap();
            count + 4
        });
        fixture.fork("changed");
        fixture.apply(
            "changed",
            vec![
                write("/work/a", b"after a\n"),
                write("/work/z", b"after z\n"),
            ],
        );
        // Each page owns its own operation demand; no previous-page counts are reused.
        let before = fixture.work();
        let first = fixture.diff("main", "changed", 1, 0);
        let first_work = fixture.since(before);
        assert_eq!(paths(&first), ["/work/a"]);
        assert!(first.get_bool("hasMore"));
        assert_eq!(first.get_int("nextPagePos"), Some(1));
        let before = fixture.work();
        let second = fixture.diff("main", "changed", 1, 1);
        let second_work = fixture.since(before);
        assert_eq!(paths(&second), ["/work/z"]);
        assert!(!second.get_bool("hasMore"));
        assert_eq!(second.get("nextPagePos"), Some(&SdkValue::Null));
        let before = fixture.work();
        let beyond = fixture.diff("main", "changed", 1, 2);
        let beyond_work = fixture.since(before);
        assert!(items(&beyond).is_empty());
        assert!(!beyond.get_bool("hasMore"));
        println!("SDK_DIFF_WIDE files={count} first={first_work:?} second={second_work:?} beyond={beyond_work:?} profile=Disposable cache=uncontrolled diagnostic_count_gate=true");
        observed.push((count, first_work, second_work, beyond_work));
        fixture.cleanup();
    }
    // Fewer than one Store batch per unchanged path rejects singleton traversal.
    // This is a demand-count regression, not a latency or cold-cache qualification.
    for (count, first, second, beyond) in observed {
        for (page, work) in [(0, first), (1, second), (2, beyond)] {
            assert!(
                work.object_batches <= count as u64,
                "wide diff page {page}: {count} files require too many Store batches: {work:?}"
            );
        }
    }
}

#[test]
fn equal_roots_require_no_namespace_scan() {
    let fixture = Fixture::new("sdk-diff-equal-root", |source| {
        for n in 0..2048 {
            fs::write(source.join(format!("item-{n:06}")), b"untouched\n").unwrap();
        }
        2048
    });
    fixture.fork("same");
    let before = fixture.work();
    let result = fixture.diff("main", "same", 1000, 0);
    let demand = fixture.since(before);
    assert!(items(&result).is_empty());
    assert!(!result.get_bool("hasMore"));
    println!("SDK_DIFF_EQUAL files=2048 work={demand:?} profile=Disposable");
    fixture.cleanup();
    assert!(
        demand.object_batches <= 32,
        "equal roots scanned namespace: {demand:?}"
    );
}

#[test]
fn diff_preserves_hardlink_identity_type_changes_and_hunk_omission() {
    let fixture = Fixture::new("sdk-diff-semantics", |source| {
        fs::create_dir(source.join("cache")).unwrap();
        fs::create_dir(source.join("work")).unwrap();
        fs::write(source.join("work/shared"), b"old shared\n").unwrap();
        fs::hard_link(source.join("work/shared"), source.join("cache/alias")).unwrap();
        fs::write(source.join("work/binary"), b"\0before").unwrap();
        fs::write(source.join("work/identity"), b"same bytes\n").unwrap();
        fs::write(source.join("work/long"), b"old\n").unwrap();
        fs::write(source.join("work/type"), b"regular\n").unwrap();
        8
    });
    fixture.fork("changed");
    fixture.apply(
        "changed",
        vec![
            write("/work/shared", b"new shared\n"),
            write("/work/binary", b"\0after"),
            operation("rm", "/work/identity"),
            write("/work/identity", b"same bytes\n"),
            write("/work/long", vec![b'x'; 48 * 1024]),
            operation("rm", "/work/type"),
            operation("mkdir", "/work/type"),
            write("/work/type/child", b"child\n"),
            write("/work/z-added", b"added\n"),
        ],
    );
    let mut all = Vec::new();
    let mut position = 0;
    for _ in 0..16 {
        let page = fixture.diff("main", "changed", 2, position);
        all.extend(items(&page).iter().cloned());
        if !page.get_bool("hasMore") {
            break;
        }
        let next = page.get_int("nextPagePos").unwrap();
        assert_eq!(next, position + items(&page).len() as i64);
        assert!(next > position);
        position = next;
    }
    let actual: Vec<_> = all
        .iter()
        .map(|item| item.get_text("path").unwrap())
        .collect();
    assert_eq!(
        actual,
        [
            "/cache/alias",
            "/work",
            "/work/binary",
            "/work/identity",
            "/work/long",
            "/work/shared",
            "/work/type",
            "/work/type/child",
            "/work/z-added"
        ]
    );
    let find = |path: &str| {
        all.iter()
            .find(|item| item.get_text("path") == Some(path))
            .unwrap()
    };
    let alias = find("/cache/alias");
    let shared = find("/work/shared");
    assert!(alias.get_bool("content") && shared.get_bool("content"));
    assert!(!alias.get_bool("identity") && !shared.get_bool("identity"));
    assert_eq!(alias.get("before"), shared.get("before"));
    assert_eq!(alias.get("after"), shared.get("after"));
    let binary = find("/work/binary");
    assert!(binary.get_bool("content"));
    assert_eq!(binary.get("hunks"), Some(&SdkValue::Null));
    assert!(!binary.get_bool("hunksOmitted"));
    let work = find("/work");
    assert!(work.get_bool("metadata"));
    assert!(!work.get_bool("identity") && !work.get_bool("content"));
    let identity = find("/work/identity");
    assert!(identity.get_bool("identity"));
    assert!(!identity.get_bool("content"));
    let long = find("/work/long");
    assert!(long.get_bool("hunksOmitted"));
    assert_eq!(long.get("hunks"), Some(&SdkValue::Null));
    let replaced = find("/work/type");
    assert_eq!(replaced.get_text("kind"), Some("Modified"));
    assert_eq!(replaced.get_text("type"), Some("dir"));
    assert!(replaced.get_bool("identity") && replaced.get_bool("content"));
    assert_eq!(find("/work/type/child").get_text("kind"), Some("Added"));
    fixture.cleanup();
}

#[test]
fn directory_metadata_is_emitted_and_counted_by_both_diff_routes() {
    let fixture = Fixture::new("sdk-diff-directory-metadata", |source| {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir(source.join("mode")).unwrap();
        fs::set_permissions(source.join("mode"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(source.join("tail"), b"before\n").unwrap();
        2
    });
    fixture.fork("changed");
    let first = fixture.apply("changed", vec![write("/tail", b"after\n")]);
    let before = first.get_text("head").unwrap().to_owned();
    let after = fixture.chmod("changed", "/mode", 0o700);
    let first_page = fixture.diff("main", "changed", 1, 0);
    assert_eq!(paths(&first_page), ["/mode"]);
    assert_eq!(first_page.get_int("nextPagePos"), Some(1));
    let directory = &items(&first_page)[0];
    assert_eq!(directory.get_text("type"), Some("dir"));
    assert!(directory.get_bool("metadata"));
    assert!(!directory.get_bool("content") && !directory.get_bool("identity"));
    let second_page = fixture.diff("main", "changed", 1, 1);
    assert_eq!(paths(&second_page), ["/tail"]);
    assert!(!second_page.get_bool("hasMore"));
    let commits = fixture.call(
        "branch.diff_commits",
        vec![
            ("from", SdkValue::text(before)),
            ("to", SdkValue::text(after)),
            ("pageSize", SdkValue::Int(1)),
            ("pagePos", SdkValue::Int(0)),
        ],
    );
    assert_eq!(paths(&commits), ["/mode"]);
    assert!(!commits.get_bool("hasMore"));
    assert_eq!(items(&commits)[0], *directory);
    fixture.cleanup();
}

#[test]
fn traversal_continues_at_64_and_65_children_in_component_order() {
    for count in [64usize, 65] {
        let fixture = Fixture::new(&format!("sdk-diff-boundary-{count}"), |source| {
            fs::create_dir(source.join("a")).unwrap();
            for n in 0..count - 1 {
                fs::write(source.join(format!("a/item-{n:06}")), b"before\n").unwrap();
            }
            fs::write(source.join("a/x"), b"before\n").unwrap();
            fs::write(source.join("a-"), b"before\n").unwrap();
            count + 2
        });
        fixture.fork("changed");
        fixture.apply(
            "changed",
            vec![
                write("/a/item-000000", b"after\n"),
                write("/a/x", b"after\n"),
                write("/a-", b"after\n"),
            ],
        );
        let first = fixture.diff("main", "changed", 2, 0);
        assert_eq!(paths(&first), ["/a/item-000000", "/a/x"]);
        assert_eq!(first.get_int("nextPagePos"), Some(2));
        let second = fixture.diff("main", "changed", 2, 2);
        assert_eq!(paths(&second), ["/a-"]);
        assert!(!second.get_bool("hasMore"));
        fixture.cleanup();
    }
}

#[test]
fn byte_limited_windows_preserve_diff_grep_and_glob_membership() {
    for count in [32usize, 65] {
        let name = |n| format!("{}{:03}", "n".repeat(252), n);
        let final_path = format!("/wide/{}", name(count - 1));
        let fixture = Fixture::new(&format!("sdk-diff-long-names-{count}"), |source| {
            fs::create_dir(source.join("wide")).unwrap();
            fs::create_dir(source.join("wide/deep")).unwrap();
            fs::write(source.join("wide/deep/child"), b"nested needle\n").unwrap();
            for n in 0..count {
                let text: &[u8] = if n + 1 == count {
                    b"needle before\n"
                } else {
                    b"untouched\n"
                };
                fs::write(source.join("wide").join(name(n)), text).unwrap();
            }
            std::os::unix::fs::symlink(name(count - 1), source.join("wide/link")).unwrap();
            count + 4
        });
        fixture.fork("changed");
        fixture.apply("changed", vec![write(&final_path, b"needle after\n")]);
        let diff = fixture.diff("main", "changed", 1, 0);
        assert_eq!(paths(&diff), [final_path.as_str()]);
        assert!(!diff.get_bool("hasMore"));
        let beyond = fixture.diff("main", "changed", 1, 1);
        assert!(items(&beyond).is_empty());
        let glob = fixture.call(
            "fs.glob",
            vec![
                ("branch", SdkValue::text("changed")),
                ("path", SdkValue::text("/wide")),
                ("pattern", SdkValue::text("wide/n*")),
                ("pageSize", SdkValue::Int(1000)),
                ("pagePos", SdkValue::Int(0)),
            ],
        );
        let expected: Vec<_> = (0..count)
            .map(|n| SdkValue::text(format!("/wide/{}", name(n))))
            .collect();
        assert_eq!(items(&glob), expected);
        assert!(!glob.get_bool("hasMore"));
        let grep = fixture.call(
            "fs.grep",
            vec![
                ("branch", SdkValue::text("changed")),
                ("path", SdkValue::text("/wide")),
                ("pattern", SdkValue::text("needle")),
                ("pageSize", SdkValue::Int(1000)),
                ("pagePos", SdkValue::Int(0)),
            ],
        );
        assert_eq!(paths(&grep), ["/wide/deep/child", final_path.as_str()]);
        assert!(!grep.get_bool("hasMore"));
        assert_eq!(grep.get_int("skipped"), Some(0));
        fixture.cleanup();
    }
}
