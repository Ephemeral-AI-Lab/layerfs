//! A method reads each argument just before the engine step that first uses
//! it, so a call with two faults is refused by the one met first, as before
//! the method files moved to layerfs-api (#352 api shape recovery). Each case
//! pairs an argument fault with a fault the engine finds earlier or later.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::Fixture;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn refused(fixture: &Fixture, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkRefusal {
    fixture
        .try_call(method, fields)
        .expect_err("the call is refused")
}

#[test]
fn the_first_fault_met_is_the_refusal() {
    let fixture = Fixture::new("method-refusal-order", |source| {
        fs::create_dir(source.join("dir")).unwrap();
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        2
    });
    fixture.fork("feat");

    // The directory is looked up before the page arguments are used.
    let r = refused(
        &fixture,
        "fs.list",
        vec![
            ("branch", t("feat")),
            ("path", t("/absent")),
            ("pageSize", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.code, "NoSuchPath");
    assert!(r.message.starts_with("/absent does not exist at feat@"));
    let r = refused(
        &fixture,
        "fs.list",
        vec![("branch", t("feat")), ("pageSize", SdkValue::Int(0))],
    );
    assert_eq!(r.message, "pageSize must be in 1..=1000");

    // The file is found before its line arguments are used.
    let r = refused(
        &fixture,
        "fs.read",
        vec![
            ("branch", t("feat")),
            ("path", t("/absent")),
            ("startLine", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.code, "NoSuchPath");
    let r = refused(
        &fixture,
        "fs.blame",
        vec![
            ("branch", t("feat")),
            ("path", t("/dir")),
            ("fromLine", SdkValue::Int(0)),
        ],
    );
    assert_eq!(
        (r.code.as_str(), r.message.as_str()),
        ("NotFile", "blame requires a regular file")
    );

    // The `from` side is resolved before `to` is read.
    let r = refused(&fixture, "branch.diff_branches", vec![("from", t("ghost"))]);
    assert_eq!(
        (r.code.as_str(), r.message.as_str()),
        ("NoSuchBranch", "Branch ghost does not exist")
    );
    // The sides are checked before the page arguments.
    let r = refused(
        &fixture,
        "branch.diff_branches",
        vec![
            ("from", t("main")),
            ("to", t("feat")),
            ("pageSize", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.message, "pageSize must be in 1..=1000");
    let r = refused(
        &fixture,
        "branch.diff_commits",
        vec![
            ("from", t("main")),
            ("to", t("feat")),
            ("pageSize", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.message, "diff_commits takes two saved Commits");

    // The left file is found before the right selector is read.
    let r = refused(
        &fixture,
        "fs.diff_files",
        vec![("left", SdkValue::map([("path", t("/absent"))]))],
    );
    assert_eq!(r.code, "NoSuchPath");

    // An empty pattern is refused before the path is checked.
    let r = refused(
        &fixture,
        "fs.grep",
        vec![
            ("branch", t("feat")),
            ("pattern", t("")),
            ("path", t("bad")),
        ],
    );
    assert_eq!(r.message, "pattern is empty");

    // The project, then the Branch, then the changes.
    let r = refused(
        &fixture,
        "fs.write",
        vec![("path", t("bad")), ("content", SdkValue::Int(1))],
    );
    assert_eq!(r.message, "missing text argument `branch`");
    let r = refused(
        &fixture,
        "fs.write",
        vec![
            ("branch", t("main")),
            ("path", t("bad")),
            ("content", SdkValue::Int(1)),
        ],
    );
    assert_eq!(r.code, "ReadOnly");
    // A write's content is checked before its path.
    let r = refused(
        &fixture,
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("bad")),
            ("content", SdkValue::Int(1)),
        ],
    );
    assert_eq!(
        (r.message.as_str(), &r.detail),
        (
            "write content must be text or bytes",
            &SdkValue::map([("index", SdkValue::Int(0))])
        )
    );

    // No bytes follow a call on the in-process route: a streamed write to a
    // new path is refused at its index and nothing is committed.
    let r = refused(
        &fixture,
        "fs.apply",
        vec![
            ("branch", t("feat")),
            (
                "changes",
                SdkValue::List(vec![SdkValue::map([
                    ("op", t("write")),
                    ("path", t("/streamed.txt")),
                    ("streamed", SdkValue::Int(5)),
                ])]),
            ),
        ],
    );
    assert_eq!(
        (r.code.as_str(), r.message.as_str(), &r.detail),
        (
            "InvalidInput",
            "streamed body beyond the declared bytes",
            &SdkValue::map([("index", SdkValue::Int(0))])
        )
    );
    let commits = fixture.call("branch.list_commits", vec![("branch", t("feat"))]);
    assert!(support::items(&commits).is_empty());
    fixture.cleanup();
}
