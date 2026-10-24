//! Permission modes on the Branch file calls (#442). `fs.write` and the
//! `write` and `mkdir` changes of `fs.apply` take an optional `mode`; a
//! `chmod` change sets one; `fs.stat` reports it. Without `mode` a new file
//! is 0o644, a new folder 0o755 and an existing entry keeps its mode. A mode
//! LayerFS cannot store is refused as `project.init` refuses it, and a
//! refused change commits nothing.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use support::Fixture;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn change(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

fn mode(f: &Fixture, branch: &str, path: &str) -> i64 {
    match f
        .call("fs.stat", vec![("branch", t(branch)), ("path", t(path))])
        .get("mode")
    {
        Some(SdkValue::Int(n)) => *n,
        other => panic!("fs.stat {path} mode: {other:?}"),
    }
}

fn commits(f: &Fixture) -> usize {
    support::items(&f.call("branch.list_commits", vec![("branch", t("feat"))])).len()
}

fn refused(f: &Fixture, changes: Vec<SdkValue>) -> SdkRefusal {
    f.try_call(
        "fs.apply",
        vec![("branch", t("feat")), ("changes", SdkValue::List(changes))],
    )
    .expect_err("the change is refused")
}

fn source(root: &std::path::Path) -> usize {
    let set = |path: &str, mode| {
        fs::set_permissions(root.join(path), fs::Permissions::from_mode(mode)).unwrap()
    };
    fs::write(root.join("tool.sh"), b"#!/bin/sh\n").unwrap();
    set("tool.sh", 0o755);
    fs::create_dir(root.join("private")).unwrap();
    fs::write(root.join("private/f"), b"f\n").unwrap();
    set("private", 0o750);
    fs::write(root.join("a.txt"), b"a\n").unwrap();
    set("a.txt", 0o644);
    std::os::unix::fs::symlink("a.txt", root.join("link")).unwrap();
    5
}

#[test]
fn write_mkdir_and_chmod_set_modes_that_stat_reports() {
    let f = Fixture::new("fs-mode", source);
    f.fork("feat");
    // Init keeps the source's modes.
    assert_eq!(mode(&f, "main", "/tool.sh"), 0o755);
    assert_eq!(mode(&f, "main", "/private"), 0o750);
    assert_eq!(mode(&f, "main", "/a.txt"), 0o644);

    // A new file gets `mode`, else 0o644.
    f.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/new.sh")),
            ("content", t("#!/bin/sh\n")),
            ("mode", SdkValue::Int(0o755)),
        ],
    );
    f.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/plain.txt")),
            ("content", t("p\n")),
        ],
    );
    assert_eq!(mode(&f, "feat", "/new.sh"), 0o755);
    assert_eq!(mode(&f, "feat", "/plain.txt"), 0o644);

    // In one apply: a binary write with a mode, a folder whose new parent
    // gets 0o755, and a sticky folder.
    f.apply(
        "feat",
        vec![
            change(vec![
                ("op", t("write")),
                ("path", t("/applied.sh")),
                ("bytes", SdkValue::Bytes(vec![0, 1, 2])),
                ("mode", SdkValue::Int(0o700)),
            ]),
            change(vec![
                ("op", t("mkdir")),
                ("path", t("/d/e")),
                ("mode", SdkValue::Int(0o750)),
            ]),
            change(vec![
                ("op", t("mkdir")),
                ("path", t("/shared")),
                ("mode", SdkValue::Int(0o1777)),
            ]),
        ],
    );
    assert_eq!(mode(&f, "feat", "/applied.sh"), 0o700);
    assert_eq!(mode(&f, "feat", "/d"), 0o755);
    assert_eq!(mode(&f, "feat", "/d/e"), 0o750);
    assert_eq!(mode(&f, "feat", "/shared"), 0o1777);

    // An existing file keeps its mode unless one is given.
    let write = |content: &str, mode: Option<i64>| {
        let mut fields = vec![
            ("branch", t("feat")),
            ("path", t("/tool.sh")),
            ("content", t(content)),
        ];
        fields.extend(mode.map(|m| ("mode", SdkValue::Int(m))));
        f.call("fs.write", fields)
    };
    write("#!/bin/sh\necho\n", None);
    assert_eq!(mode(&f, "feat", "/tool.sh"), 0o755);
    // Same bytes, new mode: a mode-only Commit.
    assert_eq!(
        write("#!/bin/sh\necho\n", Some(0o700)).get_text("kind"),
        Some("Committed")
    );
    assert_eq!(mode(&f, "feat", "/tool.sh"), 0o700);

    // An existing folder keeps its mode, as `mkdir -p -m` does.
    let kept = f.call(
        "fs.mkdir",
        vec![
            ("branch", t("feat")),
            ("path", t("/private")),
            ("mode", SdkValue::Int(0o700)),
        ],
    );
    assert_eq!(kept.get_text("kind"), Some("UpToDate"));
    assert_eq!(mode(&f, "feat", "/private"), 0o750);

    // chmod of a file and a folder; the same mode again changes nothing.
    let chmod = |path: &str, m: i64| {
        f.apply(
            "feat",
            vec![change(vec![
                ("op", t("chmod")),
                ("path", t(path)),
                ("mode", SdkValue::Int(m)),
            ])],
        )
    };
    assert_eq!(chmod("/a.txt", 0o600).get_text("kind"), Some("Committed"));
    assert_eq!(chmod("/private", 0o700).get_text("kind"), Some("Committed"));
    assert_eq!(chmod("/a.txt", 0o600).get_text("kind"), Some("UpToDate"));
    assert_eq!(mode(&f, "feat", "/a.txt"), 0o600);
    assert_eq!(mode(&f, "feat", "/private"), 0o700);
    // main is untouched.
    assert_eq!(mode(&f, "main", "/a.txt"), 0o644);

    // A Workspace chmod and Commit reads back the same way.
    f.chmod("feat", "/plain.txt", 0o640);
    assert_eq!(mode(&f, "feat", "/plain.txt"), 0o640);
    f.cleanup();
}

#[test]
fn a_mode_the_store_cannot_hold_is_refused_and_commits_nothing() {
    let f = Fixture::new("fs-mode-refusals", source);
    f.fork("feat");
    let before = commits(&f);
    let write = |m: SdkValue| {
        change(vec![
            ("op", t("write")),
            ("path", t("/x")),
            ("content", t("x")),
            ("mode", m),
        ])
    };

    // Not a permission mode at all.
    for bad in [t("755"), SdkValue::Int(-1), SdkValue::Int(0o10000)] {
        let r = refused(&f, vec![write(bad)]);
        assert_eq!(r.code, "InvalidInput");
        assert!(r.message.starts_with("mode must be an integer"), "{r:?}");
    }

    // Bits LayerFS cannot store: reason `Mode`, as in project.init.
    for (path, op, m) in [
        ("/x", "write", 0o4755),
        ("/x", "write", 0o1644),
        ("/d", "mkdir", 0o2755),
        ("/a.txt", "chmod", 0o1644),
    ] {
        let mut fields = vec![("op", t(op)), ("path", t(path)), ("mode", SdkValue::Int(m))];
        if op == "write" {
            fields.push(("content", t("x")));
        }
        // The refused change is the second: the first one is not committed.
        let r = refused(
            &f,
            vec![
                change(vec![("op", t("mkdir")), ("path", t("/first"))]),
                change(fields),
            ],
        );
        assert_eq!(r.code, "InvalidInput", "{op} {m:o}");
        assert_eq!(r.detail.get_text("reason"), Some("Mode"), "{op} {m:o}");
        assert_eq!(r.detail.get_text("path"), Some(path));
        assert_eq!(r.detail.get_text("mode"), Some(format!("{m:04o}").as_str()));
        assert_eq!(r.detail.get("index"), Some(&SdkValue::Int(1)));
    }

    // chmod needs a mode, an existing entry, and not a symlink.
    let chmod = |path: &str, m: Option<i64>| {
        let mut fields = vec![("op", t("chmod")), ("path", t(path))];
        fields.extend(m.map(|m| ("mode", SdkValue::Int(m))));
        refused(&f, vec![change(fields)])
    };
    assert_eq!(chmod("/a.txt", None).code, "InvalidInput");
    assert_eq!(chmod("/missing", Some(0o644)).code, "NoSuchPath");
    assert_eq!(chmod("/link", Some(0o644)).code, "NotFile");

    assert_eq!(commits(&f), before);
    f.cleanup();
}
