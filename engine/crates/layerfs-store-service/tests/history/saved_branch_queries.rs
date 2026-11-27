//! Saved branch reads exercise actual Normal Store history and provenance.
use crate::support;
use layerfs_protocol::sdk::SdkValue;
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
/// The newest Commit of `branch` (its head) and that Commit's base.
fn head(f: &Fixture, branch: &str) -> (String, String) {
    let listed = f.call("branch.list_commits", vec![("branch", text(branch))]);
    let newest = &items(&listed)[0];
    let commit = newest.get_text("commit").unwrap().to_owned();
    (commit, newest.get_text("base").unwrap().to_owned())
}
#[test]
fn mixed_comparisons_find_equal_text_different_owners_and_keep_result_pages() {
    let f = fixture("saved-attribution");
    f.fork("left");
    f.fork("right");
    f.apply("left", vec![write("/a", "same\n"), write("/b", "new\n")]);
    f.apply("right", vec![write("/a", "same\n")]);
    let (left, _) = head(&f, "left");
    let (right, _) = head(&f, "right");
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
fn first_commit_base_comparison_and_component_prefix_are_preserved() {
    let f = fixture("saved-first-base");
    f.fork("work");
    f.apply(
        "work",
        vec![write("/a", "after\n"), write("/ab", "other\n")],
    );
    let (cid, base) = head(&f, "work");
    let (cid, base) = (cid.as_str(), base.as_str());
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
/// as Layers with their source Branch and Commit (#611); a page goes on
/// after the last row of the page before.
#[test]
fn commit_list_on_main_pages_published_layers() {
    let f = fixture("main-commit-list");
    f.fork("work");
    f.apply("work", vec![write("/a", "one\n")]);
    let (first_commit, _) = head(&f, "work");
    f.call("branch.publish", vec![("branch", text("work"))]);
    f.apply("work", vec![write("/a", "two\n")]);
    let (second_commit, _) = head(&f, "work");
    f.call("branch.publish", vec![("branch", text("work"))]);

    let whole = f.call("branch.list_commits", vec![("branch", text("main"))]);
    assert_eq!(items(&whole).len(), 3);
    assert_eq!(whole.get("next"), Some(&SdkValue::Null));

    let one = |after: Option<&str>| {
        let mut args = vec![("branch", text("main")), ("limit", SdkValue::Int(1))];
        if let Some(after) = after {
            args.push(("after", text(after)));
        }
        f.call("branch.list_commits", args)
    };
    let page = one(None);
    assert_eq!(items(&page).len(), 1);
    let newest = items(&page)[0].clone();
    assert_eq!(page.get_text("next"), newest.get_text("commit"));
    assert_eq!(
        newest.get_text("sourceCommit"),
        Some(second_commit.as_str())
    );
    assert_eq!(newest.get_text("sourceBranch"), Some("work"));
    assert_eq!(newest.get_text("via"), Some("publish"));

    let page = one(page.get_text("next"));
    let middle = items(&page)[0].clone();
    assert_eq!(middle.get_text("sourceCommit"), Some(first_commit.as_str()));
    let SdkValue::Text(parent) = newest.get("parent").unwrap() else {
        panic!("the newest Layer names its parent")
    };
    assert_eq!(middle.get_text("commit"), Some(parent.as_str()));

    let page = one(page.get_text("next"));
    let genesis = &items(&page)[0];
    assert_eq!(genesis.get("parent"), Some(&SdkValue::Null));
    assert_eq!(genesis.get("sourceBranch"), Some(&SdkValue::Null));
    assert_eq!(genesis.get("sourceCommit"), Some(&SdkValue::Null));
    assert_eq!(genesis.get_text("via"), Some("init"));
    assert_eq!(page.get("next"), Some(&SdkValue::Null));
    f.cleanup();
}
