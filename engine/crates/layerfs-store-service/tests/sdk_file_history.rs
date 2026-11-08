//! Saved file queries over a real Disposable Store, without a daemon or UI model.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, operation, write, Fixture};

fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}
fn integer(value: i64) -> SdkValue {
    SdkValue::Int(value)
}
fn rows(value: &SdkValue) -> &[SdkValue] {
    match value.get("rows") {
        Some(SdkValue::List(rows)) => rows,
        _ => panic!("missing rows: {value:?}"),
    }
}
fn id(value: &SdkValue) -> &str {
    value.get("commit").unwrap().get_text("id").unwrap()
}
fn selector(at: &str, path: &str) -> SdkValue {
    SdkValue::map([("commit", t(at)), ("path", t(path))])
}
fn diff(f: &Fixture, left: &str, right: &str, path: &str) -> SdkValue {
    f.call(
        "fs.diff_files",
        vec![
            ("left", selector(left, path)),
            ("right", selector(right, path)),
            ("includeAttribution", SdkValue::Bool(true)),
        ],
    )
}
fn revision(
    f: &Fixture,
    branch: &str,
    path: &str,
    anchor: Option<&str>,
    page: i64,
    size: i64,
    traversal: &str,
) -> SdkValue {
    let mut args = vec![
        ("branch", t(branch)),
        ("path", t(path)),
        ("page", integer(page)),
        ("pageSize", integer(size)),
        ("traversal", t(traversal)),
    ];
    if let Some(anchor) = anchor {
        args.push(("commit", t(anchor)));
    }
    f.call("fs.revision", args)
}

#[test]
fn branch_revisions_compare_first_commit_to_base_and_pin_matching_pages() {
    let f = Fixture::new("file-revision-pages", |source| {
        fs::write(source.join("target"), "base\n").unwrap();
        1
    });
    f.fork("work");
    f.apply("work", vec![write("/other", "unrelated\n")]);
    assert!(items(&revision(&f, "work", "/target", None, 1, 2, "branch")).is_empty());
    let edited = f.apply("work", vec![write("/target", "changed\n")]);
    let edited_id = id(&edited).to_owned();
    f.chmod("work", "/target", 0o600);
    f.apply(
        "work",
        vec![operation("rm", "/target"), write("/target", "changed\n")],
    );
    f.apply("work", vec![operation("rm", "/target")]);
    let first = revision(&f, "work", "/target", None, 1, 2, "branch");
    let anchor = first.get("at").unwrap().get_text("id").unwrap().to_owned();
    assert_eq!(items(&first).len(), 2);
    assert_eq!(items(&first)[0].get_text("kind"), Some("Removed"));
    let identity = items(&first)[1].get("changes").unwrap();
    assert!(identity.get_bool("identity"));
    assert!(!identity.get_bool("content"));
    assert!(first.get_bool("hasMore"));
    f.apply("work", vec![write("/target", "new head\n")]);
    let second = revision(&f, "work", "/target", Some(&anchor), 2, 2, "branch");
    assert_eq!(items(&second).len(), 2);
    assert!(items(&second)[0]
        .get("changes")
        .unwrap()
        .get_bool("metadata"));
    let original = &items(&second)[1];
    assert_eq!(
        original.get("ref").unwrap().get_text("id"),
        Some(edited_id.as_str())
    );
    assert_eq!(original.get_text("kind"), Some("Modified"));
    assert_eq!(
        original.get("comparedTo").unwrap().get_text("kind"),
        Some("Commit")
    );
    assert!(!second.get_bool("hasMore"));
    assert_eq!(
        second,
        revision(&f, "work", "/target", Some(&anchor), 2, 2, "branch")
    );
    assert!(items(&revision(
        &f,
        "work",
        "/never",
        Some(&anchor),
        1,
        2,
        "provenance"
    ))
    .is_empty());
    f.cleanup();
}

#[test]
fn first_changed_inherited_file_is_modified_and_provenance_includes_init() {
    let f = Fixture::new("file-revision-base", |source| {
        fs::write(source.join("target"), "old\n").unwrap();
        1
    });
    f.fork("work");
    f.apply("work", vec![write("/target", "new\n")]);
    let own = revision(&f, "work", "/target", None, 1, 10, "branch");
    assert_eq!(items(&own).len(), 1);
    assert_eq!(items(&own)[0].get_text("kind"), Some("Modified"));
    assert_eq!(
        items(&own)[0].get("comparedTo").unwrap().get_text("kind"),
        Some("Layer")
    );
    let inherited = revision(&f, "work", "/target", None, 1, 10, "provenance");
    assert_eq!(items(&inherited).len(), 2);
    assert_eq!(items(&inherited)[1].get_text("kind"), Some("Added"));
    assert_eq!(
        items(&inherited)[1].get("recordedAt"),
        Some(&SdkValue::Null)
    );
    assert_eq!(
        items(&inherited)[1].get_text("timeStatus"),
        Some("unavailable")
    );
    f.cleanup();
}

#[test]
fn equal_text_independent_owners_and_publication_source_are_distinct() {
    let f = Fixture::new("file-attribution", |source| {
        fs::write(source.join("target"), "old\nkept\n").unwrap();
        1
    });
    f.fork("left");
    f.fork("right");
    let left = f.apply("left", vec![write("/target", "same\nkept\n")]);
    let right = f.apply("right", vec![write("/target", "same\nkept\n")]);
    let d = diff(&f, "left", "right", "/target");
    assert!(d.get_bool("contentEqual"));
    assert!(d.get("changes").unwrap().get_bool("attribution"));
    assert_eq!(d.get_int("attributionChanges"), Some(1));
    assert_eq!(rows(&d).len(), 2);
    let row = &rows(&d)[0];
    assert!(row.get_bool("ownerChanged"));
    assert_eq!(
        row.get("beforeOwner")
            .unwrap()
            .get("ref")
            .unwrap()
            .get_text("id"),
        Some(id(&left))
    );
    assert_eq!(
        row.get("afterOwner")
            .unwrap()
            .get("ref")
            .unwrap()
            .get_text("id"),
        Some(id(&right))
    );
    assert!(!rows(&d)[1].get_bool("ownerChanged"));
    f.call("branch.publish", vec![("branch", t("left"))]);
    let published = diff(&f, "left", "main", "/target");
    assert_eq!(
        published.get("changes").unwrap().get("attribution"),
        Some(&SdkValue::Bool(false))
    );
    assert_eq!(
        published.get("rightRef").unwrap().get_text("kind"),
        Some("Layer")
    );
    assert_eq!(
        rows(&published)[0]
            .get("afterOwner")
            .unwrap()
            .get("ref")
            .unwrap()
            .get_text("id"),
        Some(id(&left))
    );
    f.cleanup();
}

#[test]
fn equal_whole_tree_roots_do_not_shortcut_file_ownership() {
    let f = Fixture::new("file-attribution-equal-root", |source| {
        fs::write(source.join("target"), "old\n").unwrap();
        1
    });
    f.fork("left");
    f.fork("right");
    for (branch, workspace) in [("left", 201), ("right", 202)] {
        f.workspace_commit(
            branch,
            &[support::Change::Write("/target", b"new\n")],
            workspace,
            || {},
        );
    }
    let d = diff(&f, "left", "right", "/target");
    assert_eq!(
        d.get("leftRef").unwrap().get_text("root"),
        d.get("rightRef").unwrap().get_text("root")
    );
    assert!(d.get_bool("contentEqual") && d.get_bool("sameInode"));
    assert!(d.get("changes").unwrap().get_bool("attribution"));
    assert_eq!(d.get_int("attributionChanges"), Some(1));
    f.cleanup();
}

#[test]
fn missing_sides_are_opt_in_and_content_edits_are_not_attribution_only() {
    let f = Fixture::new("file-diff-missing", |source| {
        fs::write(source.join("target"), "old\n").unwrap();
        1
    });
    f.fork("work");
    f.apply("work", vec![write("/added", "hello\n")]);
    let args = vec![
        ("left", selector("main", "/added")),
        ("right", selector("work", "/added")),
    ];
    assert_eq!(
        f.try_call("fs.diff_files", args.clone()).unwrap_err().code,
        "NoSuchPath"
    );
    let mut allowed = args;
    allowed.extend([
        ("allowMissing", SdkValue::Bool(true)),
        ("includeAttribution", SdkValue::Bool(true)),
    ]);
    let added = f.call("fs.diff_files", allowed);
    assert_eq!(added.get("before"), Some(&SdkValue::Null));
    assert_eq!(added.get_text("changeKind"), Some("Added"));
    assert!(rows(&added)
        .iter()
        .all(|r| r.get_text("op") == Some("+") && !r.get_bool("ownerChanged")));
    let both = f
        .try_call(
            "fs.diff_files",
            vec![
                ("left", selector("main", "/missing")),
                ("right", selector("work", "/missing")),
                ("allowMissing", SdkValue::Bool(true)),
            ],
        )
        .unwrap_err();
    assert_eq!(both.code, "NoSuchPath");
    f.cleanup();
}

#[test]
fn binary_size_clipping_and_coarse_alignment_never_claim_exact_owner_changes() {
    let many = (0..1100).fold(String::new(), |mut text, n| {
        use std::fmt::Write;
        writeln!(text, "line{n}").unwrap();
        text
    });
    let f = Fixture::new("file-diff-coverage", |source| {
        fs::write(source.join("binary"), b"\0binary").unwrap();
        fs::write(source.join("large"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
        fs::write(source.join("long"), vec![b'x'; 70 * 1024]).unwrap();
        fs::write(source.join("many"), &many).unwrap();
        4
    });
    f.fork("work");
    let identical = diff(&f, "main", "main", "/binary");
    assert_eq!(
        identical.get("changes").unwrap().get("attribution"),
        Some(&SdkValue::Bool(false))
    );
    f.apply("work", vec![write("/other", "distinct saved identity")]);
    for (path, reason) in [("/binary", "binary"), ("/large", "text_limit")] {
        let d = diff(&f, "main", "work", path);
        assert_eq!(d.get_text("omissionReason"), Some(reason));
        assert_eq!(
            d.get("changes").unwrap().get("attribution"),
            Some(&SdkValue::Null)
        );
        assert_eq!(
            d.get("attributionCoverage").unwrap().get_text("status"),
            Some("unavailable")
        );
    }
    let read = f.call("fs.read", vec![("branch", t("work")), ("path", t("/long"))]);
    assert!(layerfs_store_service::read::file_facts::value_bytes(&read) <= 48 * 1024);
    assert!(read.get_bool("lineTruncated"));
    let too_long = f
        .try_call(
            "fs.diff_files",
            vec![
                ("left", selector("main", "/long")),
                ("right", selector("work", "/long")),
                ("includeAttribution", SdkValue::Bool(true)),
            ],
        )
        .unwrap_err();
    assert_eq!(too_long.code, "Capacity");
    let blame = f.call(
        "fs.blame",
        vec![
            ("branch", t("work")),
            ("path", t("/long")),
            ("lineCount", integer(1)),
        ],
    );
    let Some(SdkValue::List(lines)) = blame.get("lines") else {
        panic!("{blame:?}")
    };
    assert!(lines[0].get_bool("clipped"));
    assert_eq!(
        blame.get("coverage").unwrap().get_text("status"),
        Some("partial")
    );
    // Complete small byte windows can still overflow the LCS cell bound.
    let replacement = (0..1100).fold(String::new(), |mut text, n| {
        use std::fmt::Write;
        writeln!(text, "other{n}").unwrap();
        text
    });
    f.apply("work", vec![write("/many", replacement)]);
    let coarse = diff(&f, "main", "work", "/many");
    assert_eq!(
        coarse
            .get("contentCoverage")
            .unwrap()
            .get_text("comparison"),
        Some("coarse")
    );
    assert_eq!(
        coarse
            .get("attributionCoverage")
            .unwrap()
            .get_text("comparison"),
        Some("coarse")
    );
    assert_eq!(
        coarse.get("changes").unwrap().get("attribution"),
        Some(&SdkValue::Null)
    );
    f.cleanup();
}

#[test]
fn supporting_reads_return_pins_and_numbered_queries_do_not_skip_matches() {
    let f = Fixture::new("file-numbered-reads", |source| {
        for name in ["a", "b", "c"] {
            fs::write(source.join(name), format!("needle {name}\n")).unwrap();
        }
        3
    });
    let list = f.call(
        "fs.list",
        vec![
            ("branch", t("main")),
            ("path", t("/")),
            ("page", integer(1)),
            ("pageSize", integer(1)),
        ],
    );
    let pin = list.get("at").unwrap().get_text("id").unwrap().to_owned();
    assert_eq!(items(&list)[0].get_text("name"), Some("a"));
    let second = f.call(
        "fs.list",
        vec![
            ("branch", t("main")),
            ("commit", t(&pin)),
            ("path", t("/")),
            ("page", integer(2)),
            ("pageSize", integer(1)),
        ],
    );
    assert_eq!(items(&second)[0].get_text("name"), Some("b"));
    for method in ["fs.glob", "fs.grep"] {
        let pattern = if method == "fs.glob" { "*" } else { "needle" };
        let result = f.call(
            method,
            vec![
                ("branch", t("main")),
                ("commit", t(&pin)),
                ("path", t("/")),
                ("pattern", t(pattern)),
                ("page", integer(2)),
                ("pageSize", integer(1)),
            ],
        );
        assert_eq!(result.get("at").unwrap().get_text("id"), Some(pin.as_str()));
        assert_eq!(items(&result).len(), 1);
        assert!(result.get_bool("hasMore"));
    }
    for method in ["fs.read", "fs.stat"] {
        let result = f.call(
            method,
            vec![
                ("branch", t("main")),
                ("commit", t(&pin)),
                ("path", t("/a")),
            ],
        );
        assert_eq!(result.get("at").unwrap().get_text("id"), Some(pin.as_str()));
    }
    let bad = f
        .try_call(
            "fs.list",
            vec![
                ("branch", t("main")),
                ("path", t("/")),
                ("page", integer(1)),
                ("pagePos", integer(0)),
            ],
        )
        .unwrap_err();
    assert_eq!(bad.code, "InvalidInput");
    f.cleanup();
}

#[test]
fn a_bounded_blame_window_does_not_refuse_a_large_total_file() {
    let f = Fixture::new("file-blame-large-window", |source| {
        use std::io::Write;
        let mut file = fs::File::create(source.join("large")).unwrap();
        file.write_all(b"first\n").unwrap();
        let block = vec![b'x'; 1024 * 1024];
        for _ in 0..20 {
            file.write_all(&block).unwrap();
        }
        1
    });
    let result = f.call(
        "fs.blame",
        vec![
            ("branch", t("main")),
            ("path", t("/large")),
            ("lineCount", integer(1)),
        ],
    );
    let Some(SdkValue::List(lines)) = result.get("lines") else {
        panic!("{result:?}")
    };
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].get_text("text"), Some("first"));
    assert_eq!(result.get_int("nextLine"), Some(2));
    assert_eq!(
        result.get("coverage").unwrap().get_text("status"),
        Some("complete")
    );
    assert_eq!(
        lines[0]
            .get("owner")
            .unwrap()
            .get("ref")
            .unwrap()
            .get_text("kind"),
        Some("Layer")
    );
    f.cleanup();
}

#[test]
fn blame_reply_admits_duplicated_owner_messages_and_json_escaping() {
    let f = Fixture::new("file-blame-owner-budget", |source| {
        fs::write(source.join("target"), "old\n").unwrap();
        1
    });
    f.fork("work");
    f.call(
        "fs.write",
        vec![
            ("branch", t("work")),
            ("path", t("/target")),
            ("content", t(&"changed\n".repeat(80))),
            ("message", t(&"\"".repeat(4096))),
        ],
    );
    let page = f.call(
        "fs.blame",
        vec![("branch", t("work")), ("path", t("/target"))],
    );
    assert!(layerfs_store_service::read::file_facts::value_bytes(&page) <= 48 * 1024);
    let Some(SdkValue::List(lines)) = page.get("lines") else {
        panic!("{page:?}")
    };
    assert!(!lines.is_empty());
    let next = page.get_int("nextLine").unwrap();
    assert_eq!(next, lines.len() as i64 + 1);
    let Some(SdkValue::List(origins)) = page.get("origins") else {
        panic!("{page:?}")
    };
    assert_eq!(origins.len(), 1);
    f.cleanup();
}
