//! T5.6: path-aware reads (revision, blame, grep, diff, one-shot edit/cp,
//! rm/mv) and paged list/glob. A hidden path is `Removed` for a read or a
//! cp-from/edit, absent for rm/mv-from, and a visible re-creation for a write,
//! mkdir, cp-to or mv-to; a listing or glob omits it before paging and counts
//! it in one page-independent `removed_count`.
use crate::support;

use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::fs;
use support::{items, paths, write, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn remove(fixture: &Fixture, path: &str) -> SdkValue {
    fixture.call("project.hide_path", vec![("path", text(path))])
}

fn read(fixture: &Fixture, branch: &str, path: &str) -> Result<SdkValue, SdkRefusal> {
    fixture.try_call(
        "file.read",
        vec![("branch", text(branch)), ("path", text(path))],
    )
}

fn visible(fixture: &Fixture, branch: &str, path: &str) -> String {
    let value =
        read(fixture, branch, path).unwrap_or_else(|refusal| panic!("{branch}{path}: {refusal:?}"));
    value.get_text("text").unwrap_or_default().to_owned()
}

fn head(fixture: &Fixture, method: &str, fields: Vec<(&str, SdkValue)>) -> String {
    fixture
        .call(method, fields)
        .get_text("head")
        .unwrap()
        .to_owned()
}

fn hidden_frontier(refusal: &SdkRefusal) -> i64 {
    assert_eq!(refusal.code, "Removed", "{refusal:?}");
    let Some(SdkValue::List(hidden)) = refusal.detail.get("paths") else {
        panic!("no paths detail: {:?}", refusal.detail);
    };
    assert_eq!(hidden.len(), 1, "{refusal:?}");
    hidden[0].get_int("frontier_serial").unwrap()
}

#[test]
fn revision_stops_at_a_hidden_version_and_refuses_a_hidden_anchor() {
    let fixture = Fixture::new("t56-revision", |source| {
        fs::write(source.join("a"), "one\ntwo\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("work");
    fixture.apply("work", vec![write("/a", "one\nTWO\n")]);
    let second = head(
        &fixture,
        "file.write",
        vec![
            ("branch", text("work")),
            ("path", text("/a")),
            ("content", text("one\nTHREE\n")),
        ],
    );
    remove(&fixture, "/a");

    // The anchor itself is hidden.
    let refusal = fixture
        .try_call(
            "file.history",
            vec![
                ("branch", text("work")),
                ("commit", text(&second)),
                ("path", text("/a")),
            ],
        )
        .expect_err("hidden revision anchor");
    hidden_frontier(&refusal);

    // A visible re-creation above the frontier: the walk stops at the first
    // hidden version instead of skipping past it.
    let third = head(
        &fixture,
        "file.write",
        vec![
            ("branch", text("work")),
            ("path", text("/a")),
            ("content", text("four\n")),
        ],
    );
    let revision = fixture.call(
        "file.history",
        vec![("branch", text("work")), ("path", text("/a"))],
    );
    let found = items(&revision);
    assert_eq!(found.len(), 1, "only the visible re-creation: {revision:?}");
    assert_eq!(found[0].get_text("kind"), Some("Added"));
    assert_eq!(found[0].get("before"), Some(&SdkValue::Null));
    assert_eq!(
        found[0].get("ref").and_then(|r| r.get_text("id")),
        Some(third.as_str())
    );
    fixture.cleanup();
}

#[test]
fn blame_stops_at_a_hidden_version_and_refuses_a_hidden_anchor() {
    let fixture = Fixture::new("t56-blame", |source| {
        fs::write(source.join("a"), "one\ntwo\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("work");
    fixture.apply("work", vec![write("/a", "one\nTWO\n")]);
    let second = head(
        &fixture,
        "file.write",
        vec![
            ("branch", text("work")),
            ("path", text("/a")),
            ("content", text("one\nTHREE\n")),
        ],
    );
    remove(&fixture, "/a");

    let refusal = fixture
        .try_call(
            "file.blame",
            vec![
                ("branch", text("work")),
                ("commit", text(&second)),
                ("path", text("/a")),
            ],
        )
        .expect_err("hidden blame anchor");
    hidden_frontier(&refusal);

    let third = head(
        &fixture,
        "file.write",
        vec![
            ("branch", text("work")),
            ("path", text("/a")),
            ("content", text("one\nfour\n")),
        ],
    );
    let blame = fixture.call(
        "file.blame",
        vec![("branch", text("work")), ("path", text("/a"))],
    );
    let Some(SdkValue::List(lines)) = blame.get("lines") else {
        panic!("no blame lines: {blame:?}");
    };
    assert!(!lines.is_empty(), "{blame:?}");
    for line in lines {
        assert_eq!(line.get_text("commit"), Some(third.as_str()), "{blame:?}");
    }
    fixture.cleanup();
}

#[test]
fn grep_omits_hidden_hits_and_refuses_a_hidden_target() {
    let fixture = Fixture::new("t56-grep", |source| {
        fs::write(source.join("a"), "needle one\n").unwrap();
        fs::write(source.join("b"), "needle two\n").unwrap();
        2
    });
    fixture.fork("work");
    remove(&fixture, "/a");

    let refusal = fixture
        .try_call(
            "file.grep",
            vec![
                ("branch", text("work")),
                ("pattern", text("needle")),
                ("path", text("/a")),
            ],
        )
        .expect_err("hidden grep target");
    hidden_frontier(&refusal);

    // A hidden file's hits are omitted, and a paged call does not let them
    // consume the page.
    let page = fixture.call(
        "file.grep",
        vec![
            ("branch", text("work")),
            ("pattern", text("needle")),
            ("path", text("/")),
        ],
    );
    let found = items(&page);
    assert_eq!(found.len(), 1, "{page:?}");
    assert_eq!(found[0].get_text("path"), Some("/b"));
    let paged = fixture.call(
        "file.grep",
        vec![
            ("branch", text("work")),
            ("pattern", text("needle")),
            ("path", text("/")),
            ("page", SdkValue::Int(1)),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    assert_eq!(items(&paged).len(), 1, "{paged:?}");
    assert_eq!(items(&paged)[0].get_text("path"), Some("/b"));

    // A post-frontier re-creation is searched again.
    fixture.call(
        "file.write",
        vec![
            ("branch", text("work")),
            ("path", text("/a")),
            ("content", text("needle again\n")),
        ],
    );
    let again = fixture.call(
        "file.grep",
        vec![
            ("branch", text("work")),
            ("pattern", text("needle")),
            ("path", text("/")),
        ],
    );
    assert_eq!(items(&again).len(), 2, "{again:?}");
    fixture.cleanup();
}

#[test]
fn one_shot_edit_and_cp_from_a_hidden_path_refuse_removed() {
    let fixture = Fixture::new("t56-one-shot", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        fs::create_dir(source.join("d")).unwrap();
        fs::write(source.join("d/f"), "f\n").unwrap();
        4
    });
    for branch in ["rm", "mvfrom", "cpfrom", "edit", "cpto"] {
        fixture.fork(branch);
    }
    remove(&fixture, "/a");

    // edit and cp-from are `Removed`, not `NoSuchPath`.
    let refusal = fixture
        .try_call(
            "file.edit",
            vec![
                ("branch", text("edit")),
                ("path", text("/a")),
                ("oldText", text("a")),
                ("newText", text("z")),
            ],
        )
        .expect_err("hidden edit");
    hidden_frontier(&refusal);
    let refusal = fixture
        .try_call(
            "file.cp",
            vec![
                ("branch", text("cpfrom")),
                ("from", text("/a")),
                ("to", text("/copy")),
            ],
        )
        .expect_err("hidden cp source");
    hidden_frontier(&refusal);

    // rm and mv-from treat it as absent (the tree has no entry there).
    let refusal = fixture
        .try_call(
            "file.rm",
            vec![("branch", text("rm")), ("path", text("/a"))],
        )
        .expect_err("hidden rm");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "file.mv",
            vec![
                ("branch", text("mvfrom")),
                ("from", text("/a")),
                ("to", text("/moved")),
            ],
        )
        .expect_err("hidden mv source");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");

    // A never-existing path stays NoSuchPath for edit and rm.
    let refusal = fixture
        .try_call(
            "file.edit",
            vec![
                ("branch", text("edit")),
                ("path", text("/missing")),
                ("oldText", text("a")),
                ("newText", text("z")),
            ],
        )
        .expect_err("absent edit");
    assert_eq!(refusal.code, "NoSuchPath", "edit: {refusal:?}");
    let refusal = fixture
        .try_call(
            "file.rm",
            vec![("branch", text("rm")), ("path", text("/missing"))],
        )
        .expect_err("absent rm");
    assert_eq!(refusal.code, "NoSuchPath", "rm: {refusal:?}");

    // cp-to a hidden path is a visible re-creation.
    fixture.call(
        "file.cp",
        vec![
            ("branch", text("cpto")),
            ("from", text("/b")),
            ("to", text("/a")),
        ],
    );
    assert_eq!(visible(&fixture, "cpto", "/a"), "b\n");
    assert_eq!(visible(&fixture, "cpto", "/b"), "b\n");
    fixture.cleanup();
}

#[test]
fn paged_list_and_glob_skip_hidden_entries_before_paging() {
    let fixture = Fixture::new("t56-paged", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        fs::write(source.join("c"), "c\n").unwrap();
        fs::create_dir(source.join("d")).unwrap();
        fs::write(source.join("d/f"), "f\n").unwrap();
        5
    });
    fixture.fork("work");
    remove(&fixture, "/a");
    remove(&fixture, "/d");

    // A non-paged listing omits the hidden entries and counts both.
    let listed = fixture.call(
        "file.list",
        vec![("branch", text("work")), ("path", text("/"))],
    );
    assert_eq!(paths(&listed), vec!["/b", "/c"], "{listed:?}");
    assert_eq!(listed.get_int("removed_count"), Some(2));

    // Each page skips hidden entries before its page slot and reports the
    // same page-independent removed_count.
    let first = fixture.call(
        "file.list",
        vec![
            ("branch", text("work")),
            ("path", text("/")),
            ("page", SdkValue::Int(1)),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    assert_eq!(paths(&first), vec!["/b"], "{first:?}");
    assert_eq!(first.get_int("removed_count"), Some(2));
    let second = fixture.call(
        "file.list",
        vec![
            ("branch", text("work")),
            ("path", text("/")),
            ("page", SdkValue::Int(2)),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    assert_eq!(paths(&second), vec!["/c"], "{second:?}");
    assert_eq!(second.get_int("removed_count"), Some(2));

    // A legacy numeric page also counts only visible matches.
    let legacy = fixture.call(
        "file.list",
        vec![
            ("branch", text("work")),
            ("path", text("/")),
            ("pageSize", SdkValue::Int(1)),
            ("pagePos", SdkValue::Int(1)),
        ],
    );
    assert_eq!(paths(&legacy), vec!["/c"], "{legacy:?}");

    // A glob omits hidden matches and counts them on every page.
    let globbed = fixture.call(
        "file.glob",
        vec![
            ("branch", text("work")),
            ("pattern", text("/*")),
            ("path", text("/")),
        ],
    );
    let matched: Vec<&str> = match globbed.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                SdkValue::Text(name) => Some(name.as_str()),
                _ => None,
            })
            .collect(),
        other => panic!("no glob items: {other:?}"),
    };
    assert_eq!(matched, vec!["/b", "/c"], "{globbed:?}");
    assert_eq!(globbed.get_int("removed_count"), Some(2));

    let glob_first = fixture.call(
        "file.glob",
        vec![
            ("branch", text("work")),
            ("pattern", text("/*")),
            ("path", text("/")),
            ("page", SdkValue::Int(1)),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    assert_eq!(items(&glob_first).len(), 1, "{glob_first:?}");
    assert_eq!(glob_first.get_int("removed_count"), Some(2));
    let glob_second = fixture.call(
        "file.glob",
        vec![
            ("branch", text("work")),
            ("pattern", text("/*")),
            ("path", text("/")),
            ("page", SdkValue::Int(2)),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    assert_eq!(items(&glob_second).len(), 1, "{glob_second:?}");
    assert_eq!(glob_second.get_int("removed_count"), Some(2));
    fixture.cleanup();
}

#[test]
fn commit_diff_treats_a_hidden_side_as_absent_in_both_directions() {
    let fixture = Fixture::new("t56-branch-diff", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("left");
    fixture.fork("right");
    remove(&fixture, "/a");
    // `right` re-creates /a above the frontier; `left` stays at the hidden base.
    fixture.call(
        "file.write",
        vec![
            ("branch", text("right")),
            ("path", text("/a")),
            ("content", text("new\n")),
        ],
    );

    let change_of = |value: &SdkValue| -> Option<String> {
        let Some(SdkValue::List(items)) = value.get("items") else {
            return None;
        };
        items
            .iter()
            .find(|item| item.get_text("path") == Some("/a"))
            .and_then(|item| item.get_text("change"))
            .map(str::to_owned)
    };

    // A hidden `from` side is an addition, not a spurious modification or
    // removal.
    let forward = fixture.call(
        "commit.diff",
        vec![("ref", text("right")), ("from", text("left"))],
    );
    assert_eq!(change_of(&forward).as_deref(), Some("added"), "{forward:?}");

    // The reverse direction lists the same path as removed.
    let reverse = fixture.call(
        "commit.diff",
        vec![("ref", text("left")), ("from", text("right"))],
    );
    assert_eq!(
        change_of(&reverse).as_deref(),
        Some("removed"),
        "{reverse:?}"
    );
    fixture.cleanup();
}
