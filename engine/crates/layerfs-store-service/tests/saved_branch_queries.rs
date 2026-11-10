//! Saved branch reads exercise actual Disposable Store history and provenance.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, paths, write, Fixture};
fn text(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn fixture(name: &str) -> Fixture {
    Fixture::new(name, |source| {
        fs::write(source.join("a"), "base\n").unwrap();
        1
    })
}
fn history(f: &Fixture, branch: &str, page: i64, at: Option<&str>) -> SdkValue {
    let mut args = vec![
        ("branch", text(branch)),
        ("page", SdkValue::Int(page)),
        ("pageSize", SdkValue::Int(1)),
    ];
    if let Some(at) = at {
        args.push(("atCommit", text(at)));
    }
    f.call("branch.commits", args)
}
#[test]
fn publication_details_keep_source_and_parent_distinct_and_pages_pinned() {
    let f = fixture("saved-publications");
    let init = history(&f, "main", 1, None);
    let initid = init
        .get("atCommit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    assert_eq!(items(&init).len(), 1);
    assert_eq!(items(&init)[0].get("time"), Some(&SdkValue::Null));
    assert_eq!(items(&init)[0].get_text("timeStatus"), Some("unavailable"));
    f.fork("work");
    assert!(items(&history(&f, "work", 1, None)).is_empty());
    f.apply("work", vec![write("/a", "one\n")]);
    let first = history(&f, "work", 1, None);
    let cid = first
        .get("atCommit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    let source_time = items(&first)[0].get_int("time").unwrap();
    f.call("branch.publish", vec![("branch", text("work"))]);
    let published = history(&f, "main", 1, None);
    let layer = published
        .get("atCommit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    let record = &items(&published)[0];
    assert_eq!(record.get_text("sourceCommit"), Some(cid.as_str()));
    assert_eq!(record.get_text("parent"), Some(initid.as_str()));
    assert_eq!(record.get_int("sourceTime"), Some(source_time));
    assert_eq!(record.get("time"), Some(&SdkValue::Null));
    f.apply("work", vec![write("/a", "two\n")]);
    f.call("branch.publish", vec![("branch", text("work"))]);
    assert_eq!(history(&f, "main", 1, Some(&layer)), published);
    assert_eq!(
        items(&history(&f, "main", 2, Some(&layer)))[0].get_text("id"),
        Some(initid.as_str())
    );
    assert_eq!(history(&f, "work", 1, Some(&cid)), first);
    let detail = f.call(
        "branch.get_commit",
        vec![("branch", text("main")), ("commit", text(&layer))],
    );
    assert_eq!(detail.get("ref"), items(&published)[0].get("ref"));
    assert_eq!(
        detail.get("parentRef").unwrap().get_text("id"),
        Some(initid.as_str())
    );
    let branches = f.call("project.list_branches", vec![("name", text("main"))]);
    assert_eq!(
        items(&branches)[0]
            .get("savedHead")
            .unwrap()
            .get_text("kind"),
        Some("Layer")
    );
    f.cleanup();
}
#[test]
fn mixed_comparisons_find_equal_text_different_owners_and_keep_result_pages() {
    let f = fixture("saved-attribution");
    f.fork("left");
    f.fork("right");
    f.apply("left", vec![write("/a", "same\n"), write("/b", "new\n")]);
    f.apply("right", vec![write("/a", "same\n")]);
    let left = history(&f, "left", 1, None)
        .get("atCommit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    let right = history(&f, "right", 1, None)
        .get("atCommit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    let opts = |page| {
        vec![
            ("from", text("left")),
            ("to", text("right")),
            ("fromCommit", text(&left)),
            ("toCommit", text(&right)),
            ("includeAttribution", SdkValue::Bool(true)),
            ("page", SdkValue::Int(page)),
            ("pageSize", SdkValue::Int(1)),
        ]
    };
    let diff = f.call("branch.diff_branches", opts(1));
    assert_eq!(paths(&diff), ["/a"]);
    assert!(!items(&diff)[0].get_bool("content"));
    assert!(items(&diff)[0].get_bool("attribution"));
    assert_eq!(items(&diff)[0].get_int("changedOwnerLines"), Some(1));
    assert!(diff.get_bool("hasMore"));
    assert_eq!(paths(&f.call("branch.diff_branches", opts(2))), ["/b"]);
    f.call("branch.publish", vec![("branch", text("left"))]);
    let equal = f.call(
        "branch.diff_branches",
        vec![
            ("from", text("main")),
            ("to", text("left")),
            ("toCommit", text(&left)),
            ("page", SdkValue::Int(1)),
            ("includeAttribution", SdkValue::Bool(true)),
        ],
    );
    assert!(items(&equal).is_empty(), "{equal:?}");
    assert_eq!(equal.get("from").unwrap().get_text("kind"), Some("Layer"));
    assert_eq!(equal.get("to").unwrap().get_text("kind"), Some("Commit"));
    f.cleanup();
}
#[test]
fn saved_context_and_page_errors_are_refusals() {
    let f = fixture("saved-refusals");
    f.fork("left");
    f.fork("right");
    f.apply("left", vec![write("/a", "left\n")]);
    let left = history(&f, "left", 1, None)
        .get("atCommit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    assert_eq!(
        f.try_call(
            "branch.get_commit",
            vec![("branch", text("right")), ("commit", text(&left))]
        )
        .unwrap_err()
        .code,
        "NoSuchCommit"
    );
    assert_eq!(
        f.try_call(
            "branch.get_commit",
            vec![("branch", text("left")), ("commit", text(&left[..8]))]
        )
        .unwrap_err()
        .code,
        "InvalidInput"
    );
    assert_eq!(
        f.try_call(
            "branch.commits",
            vec![("branch", text("left")), ("page", SdkValue::Int(0))]
        )
        .unwrap_err()
        .code,
        "InvalidInput"
    );
    assert_eq!(
        f.try_call(
            "branch.commits",
            vec![
                ("branch", text("left")),
                ("page", SdkValue::Int(1)),
                ("pagePos", SdkValue::Int(0))
            ]
        )
        .unwrap_err()
        .code,
        "InvalidInput"
    );
    let beyond = f.call(
        "branch.commits",
        vec![
            ("branch", text("left")),
            ("page", SdkValue::Int(10001)),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    assert!(items(&beyond).is_empty());
    f.cleanup();
}

#[test]
fn ordinary_default_history_page_fits_and_preserves_fifty_matching_records() {
    let f = fixture("saved-default-page");
    f.fork("work");
    for n in 0..51 {
        f.apply("work", vec![write("/a", format!("value {n}\n"))]);
    }
    let first = f.call("branch.commits", vec![("branch", text("work"))]);
    assert_eq!(items(&first).len(), 50);
    assert!(first.get_bool("hasMore"));
    assert!(items(&first)
        .iter()
        .all(|item| item.get_bool("detailsOmitted")));
    let start = first.get("atCommit").unwrap().get_text("id").unwrap();
    let second = f.call(
        "branch.commits",
        vec![
            ("branch", text("work")),
            ("atCommit", text(start)),
            ("page", SdkValue::Int(2)),
        ],
    );
    assert_eq!(items(&second).len(), 1);
    assert!(!second.get_bool("hasMore"));
    f.cleanup();
}

#[test]
fn first_commit_base_comparison_and_component_prefix_are_preserved() {
    let f = fixture("saved-first-base");
    f.fork("work");
    f.apply(
        "work",
        vec![write("/a", "after\n"), write("/ab", "other\n")],
    );
    let record = history(&f, "work", 1, None);
    let cid = record.get("atCommit").unwrap().get_text("id").unwrap();
    let base = items(&record)[0].get_text("base").unwrap();
    let diff = f.call(
        "branch.diff_branches",
        vec![
            ("from", text("work")),
            ("to", text("work")),
            ("fromCommit", text(base)),
            ("toCommit", text(cid)),
            ("pathPrefix", text("/a")),
            ("page", SdkValue::Int(1)),
        ],
    );
    assert_eq!(paths(&diff), ["/a"]);
    assert_eq!(diff.get("from").unwrap().get_text("kind"), Some("Layer"));
    assert_eq!(diff.get("to").unwrap().get_text("kind"), Some("Commit"));
    let backwards = f.call(
        "branch.diff_branches",
        vec![
            ("from", text("work")),
            ("to", text("work")),
            ("fromCommit", text(cid)),
            ("toCommit", text(base)),
            ("page", SdkValue::Int(1)),
        ],
    );
    let removed = items(&backwards)
        .iter()
        .find(|i| i.get_text("path") == Some("/ab"))
        .unwrap();
    assert_eq!(removed.get("after"), Some(&SdkValue::Null));
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    f.cleanup();
}

#[test]
fn identical_saved_endpoint_needs_no_binary_attribution_guess() {
    let f = Fixture::new("saved-equal-binary", |source| {
        fs::write(source.join("binary"), [0u8, 255, 0, 128]).unwrap();
        1
    });
    let result = f.call(
        "branch.diff_branches",
        vec![
            ("from", text("main")),
            ("to", text("main")),
            ("includeAttribution", SdkValue::Bool(true)),
            ("page", SdkValue::Int(1)),
        ],
    );
    assert!(items(&result).is_empty(), "{result:?}");
    assert!(result.get("coverage").unwrap().get_bool("complete"));
    assert_eq!(result.get("scan").unwrap().get_int("examined"), Some(0));
    f.cleanup();
}

#[test]
fn unknown_prefix_and_lookahead_do_not_change_returned_page_coverage() {
    let f = Fixture::new("saved-page-coverage", |source| {
        fs::write(source.join("a"), [0u8, 255, 0, 128]).unwrap();
        fs::write(source.join("b"), "base\n").unwrap();
        2
    });
    f.fork("left");
    f.fork("right");
    f.apply("left", vec![write("/b", "same\n")]);
    f.apply("right", vec![write("/b", "same\n")]);
    let query = |page| {
        vec![
            ("from", text("left")),
            ("to", text("right")),
            ("includeAttribution", SdkValue::Bool(true)),
            ("page", SdkValue::Int(page)),
            ("pageSize", SdkValue::Int(1)),
        ]
    };
    let cold = f.call("branch.diff_branches", query(2));
    assert_eq!(paths(&cold), ["/b"]);
    assert!(cold.get("coverage").unwrap().get_bool("complete"));
    let first = f.call("branch.diff_branches", query(1));
    assert_eq!(paths(&first), ["/a"]);
    assert!(!first.get("coverage").unwrap().get_bool("complete"));
    let warm = f.call("branch.diff_branches", query(2));
    assert_eq!(warm.get("items"), cold.get("items"));
    assert_eq!(warm.get("coverage"), cold.get("coverage"));
    assert_eq!(warm.get("scan").unwrap().get_int("examined"), Some(1));
    f.cleanup();
}

#[test]
fn oversized_saved_merge_details_refuse_before_wire_encoding() {
    let f = fixture("saved-detail-budget");
    f.fork("left");
    f.fork("right");
    f.apply("left", vec![write("/a", "left\n")]);
    f.apply("right", vec![write("/b", "right\n")]);
    let merged = f.call(
        "branch.merge",
        vec![
            ("branch", text("left")),
            ("with", text("right")),
            ("name", text("merged")),
            (
                "units",
                SdkValue::List(vec![SdkValue::map([
                    ("name", SdkValue::text("u".repeat(48 * 1024))),
                    ("patterns", SdkValue::List(vec![text("/not-present")])),
                ])]),
            ),
        ],
    );
    let id = merged.get_text("head").unwrap();
    let refusal = f
        .try_call(
            "branch.get_commit",
            vec![("branch", text("merged")), ("commit", text(id))],
        )
        .unwrap_err();
    assert_eq!(refusal.code, "Capacity");
    let summary = history(&f, "merged", 1, None);
    assert_eq!(items(&summary)[0].get_text("id"), Some(id));
    assert!(items(&summary)[0].get_bool("detailsOmitted"));
    f.cleanup();
}

#[test]
fn requested_directory_attribution_remains_unknown_on_metadata_changes() {
    let f = Fixture::new("saved-directory-owner", |source| {
        fs::create_dir(source.join("folder")).unwrap();
        1
    });
    f.fork("work");
    f.chmod("work", "/folder", 0o700);
    let result = f.call(
        "branch.diff_branches",
        vec![
            ("from", text("main")),
            ("to", text("work")),
            ("includeAttribution", SdkValue::Bool(true)),
            ("page", SdkValue::Int(1)),
        ],
    );
    assert_eq!(paths(&result), ["/folder"]);
    let item = &items(&result)[0];
    assert!(item.get_bool("metadata"));
    assert_eq!(item.get("attribution"), Some(&SdkValue::Null));
    assert_eq!(
        item.get("attributionCoverage").unwrap().get_text("status"),
        Some("unavailable")
    );
    assert!(!result.get("coverage").unwrap().get_bool("complete"));
    f.cleanup();
}

/// `commit.list` on main pages the stack's published Layers, newest first,
/// as Layers with their source Branch and Commit (#611).
#[test]
fn commit_list_on_main_pages_published_layers() {
    let f = fixture("main-commit-list");
    f.fork("work");
    f.apply("work", vec![write("/a", "one\n")]);
    let first = f.call("branch.list_commits", vec![("branch", text("work"))]);
    let first_commit = items(&first)[0].get_text("id").unwrap().to_owned();
    f.call("branch.publish", vec![("branch", text("work"))]);
    f.apply("work", vec![write("/a", "two\n")]);
    let second = f.call("branch.list_commits", vec![("branch", text("work"))]);
    let second_commit = items(&second)[0].get_text("id").unwrap().to_owned();
    f.call("branch.publish", vec![("branch", text("work"))]);

    let whole = f.call("branch.list_commits", vec![("branch", text("main"))]);
    assert_eq!(items(&whole).len(), 3);

    let page = f.call(
        "branch.list_commits",
        vec![("branch", text("main")), ("pageSize", SdkValue::Int(1))],
    );
    assert_eq!(items(&page).len(), 1);
    assert_eq!(page.get("hasMore"), Some(&SdkValue::Bool(true)));
    assert_eq!(page.get("nextPagePos"), Some(&SdkValue::Int(1)));
    let newest = &items(&page)[0];
    assert_eq!(
        newest.get_text("sourceCommit"),
        Some(second_commit.as_str())
    );
    assert_eq!(newest.get_text("sourceBranch"), Some("work"));

    let page = f.call(
        "branch.list_commits",
        vec![
            ("branch", text("main")),
            ("pageSize", SdkValue::Int(1)),
            ("pagePos", SdkValue::Int(1)),
        ],
    );
    let middle = &items(&page)[0];
    assert_eq!(middle.get_text("sourceCommit"), Some(first_commit.as_str()));
    let SdkValue::Text(parent) = newest.get("parent").unwrap() else {
        panic!("the newest Layer names its parent")
    };
    assert_eq!(middle.get_text("id"), Some(parent.as_str()));

    let page = f.call(
        "branch.list_commits",
        vec![
            ("branch", text("main")),
            ("pageSize", SdkValue::Int(1)),
            ("pagePos", SdkValue::Int(2)),
        ],
    );
    let genesis = &items(&page)[0];
    assert_eq!(genesis.get("parent"), Some(&SdkValue::Null));
    assert_eq!(genesis.get("sourceBranch"), Some(&SdkValue::Null));
    assert_eq!(genesis.get("sourceCommit"), Some(&SdkValue::Null));
    assert_eq!(page.get("hasMore"), Some(&SdkValue::Bool(false)));
    assert_eq!(page.get("nextPagePos"), Some(&SdkValue::Null));
    f.cleanup();
}
