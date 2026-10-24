//! A file write names what is in its way and what is wrong with its input as
//! the read calls do: a directory is `IsDirectory`, any other non-file
//! `NotFile`, and a name the name grammar refuses is `InvalidInput`, not an
//! engine failure. Nothing is committed.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::Fixture;

fn refused(fixture: &Fixture, method: &str, path: &str) -> SdkRefusal {
    let mut fields = vec![
        ("branch", SdkValue::text("feat")),
        ("path", SdkValue::text(path)),
    ];
    if method == "fs.write" {
        fields.push(("content", SdkValue::text("x")));
    }
    fixture
        .try_call(method, fields)
        .expect_err("the call is refused")
}

#[test]
fn a_write_refusal_names_its_cause() {
    let fixture = Fixture::new("write-refusals", |source| {
        fs::create_dir(source.join("dir")).unwrap();
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        std::os::unix::fs::symlink("a.txt", source.join("link")).unwrap();
        3
    });
    fixture.fork("feat");

    // The read calls already tell the two apart.
    assert_eq!(refused(&fixture, "fs.read", "/link").code, "NotFile");

    assert_eq!(refused(&fixture, "fs.write", "/dir").code, "IsDirectory");
    let link = refused(&fixture, "fs.write", "/link");
    assert_eq!(
        (link.code.as_str(), link.message.as_str()),
        ("NotFile", "expected a regular file")
    );

    // A `\` or a name over 255 bytes passes the path check and is refused by
    // the name grammar, on the way to the entry and at the entry itself.
    let long = format!("/{}", "n".repeat(256));
    for (method, path) in [
        ("fs.write", "/a\\b"),
        ("fs.write", "/dir/a\\b"),
        ("fs.write", long.as_str()),
        ("fs.mkdir", "/a\\b"),
        ("fs.rm", "/a\\b"),
    ] {
        let r = refused(&fixture, method, path);
        assert_eq!(
            (r.code.as_str(), r.message.as_str()),
            ("InvalidInput", format!("`{path}` is not valid").as_str()),
            "{method} {path}"
        );
    }
    // The read route answers the same input the same way.
    assert_eq!(refused(&fixture, "fs.stat", "/a\\b").code, "InvalidInput");

    let commits = fixture.call(
        "branch.list_commits",
        vec![("branch", SdkValue::text("feat"))],
    );
    assert!(support::items(&commits).is_empty());
    fixture.cleanup();
}

/// A name below a file or a symlink (which is not followed) is refused
/// `NotDirectory` at that entry by every write call, as `fs.mkdir` answers.
/// Such a call keeps no Store call Workspace: more of them than the Store
/// has call Workspaces leave a valid write working. `fs.mkdir` never took
/// one, so only the other calls count toward that number.
#[test]
fn a_write_through_a_non_directory_is_not_directory() {
    let fixture = Fixture::new("write-through-non-directory", |source| {
        fs::create_dir(source.join("dir")).unwrap();
        fs::write(source.join("dir/inner.txt"), b"inner\n").unwrap();
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        std::os::unix::fs::symlink("dir", source.join("dlink")).unwrap();
        4
    });
    fixture.fork("feat");
    let t = SdkValue::text;
    let cases = [
        (
            "fs.write",
            vec![("path", t("/a.txt/x")), ("content", t("x"))],
            "/a.txt",
        ),
        (
            "fs.write",
            vec![("path", t("/dlink/x")), ("content", t("x"))],
            "/dlink",
        ),
        ("fs.rm", vec![("path", t("/a.txt/x"))], "/a.txt"),
        ("fs.mkdir", vec![("path", t("/a.txt/x"))], "/a.txt"),
        (
            "fs.edit",
            vec![
                ("path", t("/dlink/inner.txt")),
                ("oldText", t("inner")),
                ("newText", t("x")),
            ],
            "/dlink",
        ),
        (
            "fs.cp",
            vec![("from", t("/a.txt")), ("to", t("/dlink/c.txt"))],
            "/dlink",
        ),
        (
            "fs.mv",
            vec![("from", t("/dir/inner.txt")), ("to", t("/a.txt/y"))],
            "/a.txt",
        ),
    ];
    let calls = layerfs_store_service::OwnerConfig::default().namespaces + 1;
    let held_a_workspace = cases.iter().filter(|(method, ..)| *method != "fs.mkdir");
    for (method, fields, at) in cases.iter().chain(held_a_workspace.cycle().take(calls)) {
        let mut fields = fields.clone();
        fields.push(("branch", t("feat")));
        let r = fixture
            .try_call(method, fields)
            .expect_err("the call is refused");
        assert_eq!(
            (r.code.as_str(), r.message.as_str()),
            ("NotDirectory", *at),
            "{method}"
        );
    }
    fixture.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/after.txt")),
            ("content", t("after")),
        ],
    );
    fixture.cleanup();
}
