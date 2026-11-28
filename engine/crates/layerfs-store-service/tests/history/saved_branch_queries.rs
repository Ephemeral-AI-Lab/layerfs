//! Saved Branch reads over actual Normal Store history.
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
    let listed = f.call("commit.list", vec![("branch", text(branch))]);
    let newest = &items(&listed)[0];
    let commit = newest.get_text("commit").unwrap().to_owned();
    (commit, newest.get_text("base").unwrap().to_owned())
}
#[test]
fn first_commit_base_comparison_and_component_prefix_are_preserved() {
    let f = fixture("saved-first-base");
    f.fork("work");
    f.apply(
        "work",
        vec![write("/a", "after\n"), write("/ab", "other\n")],
    );
    let (cid, _) = head(&f, "work");
    // The first Commit compares from its base; `/a` scopes by component, so
    // `/ab` is outside it.
    let diff = f.call(
        "commit.diff",
        vec![("ref", text(&cid)), ("path", text("/a"))],
    );
    assert_eq!(paths(&diff), ["/a"]);
    // main still stands at the base: from the Commit back to it, `/ab` is
    // removed.
    let backwards = f.call(
        "commit.diff",
        vec![("ref", text("main")), ("from", text(&cid))],
    );
    let removed = items(&backwards)
        .iter()
        .find(|i| i.get_text("path") == Some("/ab"))
        .unwrap();
    assert_eq!(removed.get_text("change"), Some("removed"));
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

    let whole = f.call("commit.list", vec![("branch", text("main"))]);
    assert_eq!(items(&whole).len(), 3);
    assert_eq!(whole.get("next"), Some(&SdkValue::Null));

    let one = |after: Option<&str>| {
        let mut args = vec![("branch", text("main")), ("limit", SdkValue::Int(1))];
        if let Some(after) = after {
            args.push(("after", text(after)));
        }
        f.call("commit.list", args)
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
