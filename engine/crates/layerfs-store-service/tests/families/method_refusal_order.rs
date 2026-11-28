//! A method reads each argument just before the engine step that first uses
//! it, so a call with two faults is refused by the one met first, as before
//! the method files moved to layerfs-api (#352 api shape recovery). Each case
//! pairs an argument fault with a fault the engine finds earlier or later.
use crate::support;

use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
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
        "file.list",
        vec![
            ("branch", t("feat")),
            ("path", t("/absent")),
            ("pageSize", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.code, "NoSuchPath");
    assert!(r.message.starts_with("/absent does not exist at feat@"));
    // the label's short id is a prefix a Commit lookup accepts
    let label = r.message.rsplit('@').next().unwrap();
    assert_eq!(label.len(), 14);
    fixture.call("commit.get", vec![("commit", t(label))]);
    let r = refused(
        &fixture,
        "file.list",
        vec![("branch", t("feat")), ("pageSize", SdkValue::Int(0))],
    );
    assert_eq!(r.message, "pageSize must be in 1..=1000");

    // The file is found before its line arguments are used.
    let r = refused(
        &fixture,
        "file.read",
        vec![
            ("branch", t("feat")),
            ("path", t("/absent")),
            ("startLine", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.code, "NoSuchPath");
    let r = refused(
        &fixture,
        "file.blame",
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

    // `ref` is resolved before `from` is read.
    let r = refused(
        &fixture,
        "commit.diff",
        vec![("ref", t("ghost")), ("from", t("nowhere"))],
    );
    assert_eq!(
        (r.code.as_str(), r.message.as_str()),
        ("NoSuchBranch", "Branch ghost does not exist")
    );
    // The sides are checked before the page arguments.
    let r = refused(
        &fixture,
        "commit.diff",
        vec![
            ("ref", t("feat")),
            ("from", t("ghost")),
            ("limit", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.code, "NoSuchBranch");
    let r = refused(
        &fixture,
        "commit.diff",
        vec![
            ("ref", t("feat")),
            ("from", t("main")),
            ("limit", SdkValue::Int(0)),
        ],
    );
    assert_eq!(r.code, "InvalidInput");

    // An empty pattern is refused before the path is checked.
    let r = refused(
        &fixture,
        "file.grep",
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
        "file.write",
        vec![("path", t("bad")), ("content", SdkValue::Int(1))],
    );
    assert_eq!(r.message, "missing text argument `branch`");
    let r = refused(
        &fixture,
        "file.write",
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
        "file.write",
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
        "file.apply",
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
    let commits = fixture.call("commit.list", vec![("branch", t("feat"))]);
    assert!(support::items(&commits).is_empty());
    fixture.cleanup();
}

/// A hex-shaped snapshot name of 8 or more characters that is no Branch is
/// an identity fault, not a missing Branch (#611): a truncated or
/// wrong-length identity never reaches the Branch index as a name. A
/// shorter hex name stays a Branch name, and the Branch is resolved before
/// the page arguments, as always.
#[test]
fn a_short_identity_is_not_a_missing_branch() {
    let fixture = Fixture::new("short-identity", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let listed = fixture.call("commit.list", vec![("branch", t("main"))]);
    let id = support::items(&listed)[0]
        .get_text("commit")
        .unwrap()
        .to_owned();
    // A full identity reads as a snapshot.
    fixture.call("file.list", vec![("branch", t(&id))]);
    let truncated = |n: usize| id[..n].to_owned();
    let r = refused(&fixture, "file.list", vec![("branch", t(&truncated(12)))]);
    assert_eq!(r.code, "InvalidInput");
    assert_eq!(
        r.message,
        format!(
            "`{}` is not a Branch, and not a full Commit or Layer identity (66 hex characters)",
            truncated(12)
        )
    );
    // An odd-length truncation is identity-shaped too, not only an even one.
    let r = refused(&fixture, "file.list", vec![("branch", t(&truncated(65)))]);
    assert_eq!(r.code, "InvalidInput");
    // A hex name shorter than a commit.get prefix stays a Branch name.
    let r = refused(&fixture, "file.list", vec![("branch", t("cafe"))]);
    assert_eq!(r.code, "NoSuchBranch");
    // A non-hex name stays a Branch refusal.
    let r = refused(&fixture, "file.list", vec![("branch", t("ghost"))]);
    assert_eq!(r.code, "NoSuchBranch");
    // The Branch is resolved before the page arguments are read.
    let r = refused(
        &fixture,
        "commit.list",
        vec![("branch", t("ghost")), ("limit", SdkValue::Int(0))],
    );
    assert_eq!(r.code, "NoSuchBranch");
    fixture.cleanup();
}
