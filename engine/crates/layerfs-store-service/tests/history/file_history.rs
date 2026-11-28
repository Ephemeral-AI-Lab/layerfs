//! Saved file queries over a real Normal Store, without a daemon or UI model.
use crate::support;

use layerfs_protocol::sdk::SdkValue;
use std::fs;
use support::{items, operation, write, Fixture};

fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}
fn integer(value: i64) -> SdkValue {
    SdkValue::Int(value)
}
fn id(value: &SdkValue) -> &str {
    value.get("commit").unwrap().get_text("id").unwrap()
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
    f.call("file.history", args)
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
fn supporting_reads_return_pins_and_numbered_queries_do_not_skip_matches() {
    let f = Fixture::new("file-numbered-reads", |source| {
        for name in ["a", "b", "c"] {
            fs::write(source.join(name), format!("needle {name}\n")).unwrap();
        }
        3
    });
    let list = f.call(
        "file.list",
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
        "file.list",
        vec![
            ("branch", t("main")),
            ("commit", t(&pin)),
            ("path", t("/")),
            ("page", integer(2)),
            ("pageSize", integer(1)),
        ],
    );
    assert_eq!(items(&second)[0].get_text("name"), Some("b"));
    for method in ["file.glob", "file.grep"] {
        let pattern = if method == "file.glob" { "*" } else { "needle" };
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
    for method in ["file.read", "file.stat"] {
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
            "file.list",
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
        "file.blame",
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
        "file.write",
        vec![
            ("branch", t("work")),
            ("path", t("/target")),
            ("content", t(&"changed\n".repeat(80))),
            ("message", t(&"\"".repeat(4096))),
        ],
    );
    let page = f.call(
        "file.blame",
        vec![("branch", t("work")), ("path", t("/target"))],
    );
    assert!(layerfs_store_service::filesystem::facts::value_bytes(&page) <= 48 * 1024);
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
