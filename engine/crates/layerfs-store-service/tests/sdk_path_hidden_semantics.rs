//! T5.8: hidden `rm`/`mv` sources and the glob `removed_count`.
//!
//! §4.3 says a `file.mv` from a hidden path is `NotFound` and a `file.rm` of
//! a hidden path is `NotFound`. This method table spells a missing path
//! `NoSuchPath`, so those two calls report exactly what a never-existing path
//! reports; `cp`-from and `edit` consult the base view and answer `Removed`.
//! A listing counts the hidden entries among its direct results; a glob counts
//! every hidden entry the pattern would return anywhere under its query root.
//! Both totals are page-independent and `removed_count` is on every page.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::{items, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn remove(fixture: &Fixture, path: &str) -> SdkValue {
    fixture.call("project.remove_file", vec![("path", text(path))])
}

fn read(fixture: &Fixture, branch: &str, path: &str) -> Result<SdkValue, SdkRefusal> {
    fixture.try_call(
        "fs.read",
        vec![("branch", text(branch)), ("path", text(path))],
    )
}

/// The one `Removed` detail path and its frontier.
fn hidden_path(refusal: &SdkRefusal, path: &str) -> i64 {
    assert_eq!(refusal.code, "Removed", "{refusal:?}");
    let Some(SdkValue::List(hidden)) = refusal.detail.get("paths") else {
        panic!("no paths detail: {:?}", refusal.detail);
    };
    assert_eq!(hidden.len(), 1, "{refusal:?}");
    assert_eq!(hidden[0].get_text("path"), Some(path), "{refusal:?}");
    hidden[0].get_int("frontier_serial").unwrap()
}

fn glob(fixture: &Fixture, pattern: &str, path: &str) -> SdkValue {
    fixture.call(
        "fs.glob",
        vec![
            ("branch", text("work")),
            ("pattern", text(pattern)),
            ("path", text(path)),
        ],
    )
}

fn paged_glob(fixture: &Fixture, pattern: &str, page: i64, page_size: i64) -> SdkValue {
    fixture.call(
        "fs.glob",
        vec![
            ("branch", text("work")),
            ("pattern", text(pattern)),
            ("path", text("/")),
            ("page", SdkValue::Int(page)),
            ("pageSize", SdkValue::Int(page_size)),
        ],
    )
}

fn matched(value: &SdkValue) -> Vec<&str> {
    items(value)
        .iter()
        .map(|item| match item {
            SdkValue::Text(text) => text.as_str(),
            other => panic!("glob item is not a path: {other:?}"),
        })
        .collect()
}

#[test]
fn hidden_rm_and_mv_sources_are_absent_while_cp_and_edit_are_removed() {
    let fixture = Fixture::new("t58-hidden-writes", |source| {
        fs::write(source.join("a.txt"), "a\n").unwrap();
        fs::write(source.join("b.txt"), "b\n").unwrap();
        fs::create_dir(source.join("d")).unwrap();
        fs::write(source.join("d/f.txt"), "f\n").unwrap();
        4
    });
    for branch in ["rm", "mv", "cp", "edit"] {
        fixture.fork(branch);
    }
    remove(&fixture, "/a.txt");
    remove(&fixture, "/d");

    // §4.3's `NotFound` for a hidden `rm`/`mv`-from is this wire spelling:
    // those calls read the private overlay, where a hidden path is already a
    // whiteout, and answer like a never-existing path (`NoSuchPath`).
    let refusal = fixture
        .try_call(
            "fs.rm",
            vec![("branch", text("rm")), ("path", text("/a.txt"))],
        )
        .expect_err("hidden rm");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "fs.mv",
            vec![
                ("branch", text("mv")),
                ("from", text("/a.txt")),
                ("to", text("/moved.txt")),
            ],
        )
        .expect_err("hidden mv source");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");

    // A hidden directory and its descendants are absent to `rm`/`mv` too.
    let refusal = fixture
        .try_call(
            "fs.rm",
            vec![("branch", text("rm")), ("path", text("/d"))],
        )
        .expect_err("hidden directory rm");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "fs.rm",
            vec![("branch", text("rm")), ("path", text("/d/f.txt"))],
        )
        .expect_err("hidden child rm");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "fs.mv",
            vec![
                ("branch", text("mv")),
                ("from", text("/d/f.txt")),
                ("to", text("/moved.txt")),
            ],
        )
        .expect_err("hidden child mv source");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");

    // `cp`-from and `edit` consult the base tree, so a hidden source is
    // `Removed` with the governing path and frontier, not `NoSuchPath`.
    let refusal = fixture
        .try_call(
            "fs.edit",
            vec![
                ("branch", text("edit")),
                ("path", text("/a.txt")),
                ("oldText", text("a")),
                ("newText", text("z")),
            ],
        )
        .expect_err("hidden edit");
    hidden_path(&refusal, "/a.txt");
    let refusal = fixture
        .try_call(
            "fs.edit",
            vec![
                ("branch", text("edit")),
                ("path", text("/d/f.txt")),
                ("oldText", text("f")),
                ("newText", text("z")),
            ],
        )
        .expect_err("hidden child edit");
    hidden_path(&refusal, "/d/f.txt");
    let refusal = fixture
        .try_call(
            "fs.cp",
            vec![
                ("branch", text("cp")),
                ("from", text("/a.txt")),
                ("to", text("/copy.txt")),
            ],
        )
        .expect_err("hidden cp source");
    hidden_path(&refusal, "/a.txt");
    let refusal = fixture
        .try_call(
            "fs.cp",
            vec![
                ("branch", text("cp")),
                ("from", text("/d")),
                ("to", text("/copy")),
            ],
        )
        .expect_err("hidden cp directory");
    hidden_path(&refusal, "/d");

    // A genuinely absent path is still `NoSuchPath` for `cp`/`edit`: the
    // hidden and the absent cases stay distinguishable where the spec asks.
    let refusal = fixture
        .try_call(
            "fs.edit",
            vec![
                ("branch", text("edit")),
                ("path", text("/missing")),
                ("oldText", text("a")),
                ("newText", text("z")),
            ],
        )
        .expect_err("absent edit");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "fs.cp",
            vec![
                ("branch", text("cp")),
                ("from", text("/missing")),
                ("to", text("/copy")),
            ],
        )
        .expect_err("absent cp source");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");

    // A refused call leaves the branch and the destination untouched.
    assert_eq!(
        read(&fixture, "cp", "/copy.txt").unwrap_err().code,
        "NoSuchPath"
    );
    assert_eq!(
        read(&fixture, "mv", "/moved.txt").unwrap_err().code,
        "NoSuchPath"
    );
    assert_eq!(
        read(&fixture, "rm", "/b.txt").unwrap().get_text("text"),
        Some("b\n")
    );
    fixture.cleanup();
}

#[test]
fn glob_removed_count_counts_the_hidden_results_at_any_depth_on_every_page() {
    let fixture = Fixture::new("t58-glob-count", |source| {
        fs::write(source.join("a.txt"), "a\n").unwrap();
        fs::write(source.join("b.txt"), "b\n").unwrap();
        fs::create_dir(source.join("d")).unwrap();
        fs::write(source.join("d/f.txt"), "f\n").unwrap();
        fs::create_dir(source.join("d/sub")).unwrap();
        fs::write(source.join("d/sub/g.txt"), "g\n").unwrap();
        fs::create_dir(source.join("e")).unwrap();
        fs::write(source.join("e/h.txt"), "h\n").unwrap();
        8
    });
    fixture.fork("work");
    remove(&fixture, "/a.txt");
    remove(&fixture, "/d");

    // `/*` returns direct children only: the hidden file and directory are
    // omitted and counted, but the hidden entries below `/d` are not results
    // of this pattern and are not counted.
    let top = glob(&fixture, "/*", "/");
    assert_eq!(matched(&top), vec!["/b.txt", "/e"], "{top:?}");
    assert_eq!(top.get_int("removed_count"), Some(2), "{top:?}");

    // `/**` returns the whole queried subtree: every hidden entry the pattern
    // would return counts, including `/d`'s descendants at any depth.
    let all = glob(&fixture, "/**", "/");
    assert_eq!(matched(&all), vec!["/b.txt", "/e", "/e/h.txt"], "{all:?}");
    assert_eq!(all.get_int("removed_count"), Some(5), "{all:?}");

    // A pattern that names hidden descendants counts those two, not the
    // hidden directory itself (which this pattern does not match).
    let below = glob(&fixture, "/d/*", "/");
    assert!(matched(&below).is_empty(), "{below:?}");
    assert_eq!(below.get_int("removed_count"), Some(2), "{below:?}");

    // `removed_count` is present at 0 when the pattern matches no hidden entry.
    let visible_only = glob(&fixture, "/b*", "/");
    assert_eq!(matched(&visible_only), vec!["/b.txt"], "{visible_only:?}");
    assert_eq!(
        visible_only.get_int("removed_count"),
        Some(0),
        "{visible_only:?}"
    );

    // The count is page-independent: every page of the same glob reports the
    // same total while its items carry only the visible matches.
    let first = paged_glob(&fixture, "/**", 1, 2);
    assert_eq!(matched(&first), vec!["/b.txt", "/e"], "{first:?}");
    assert_eq!(first.get_int("removed_count"), Some(5), "{first:?}");
    let second = paged_glob(&fixture, "/**", 2, 2);
    assert_eq!(matched(&second), vec!["/e/h.txt"], "{second:?}");
    assert_eq!(second.get_int("removed_count"), Some(5), "{second:?}");
    fixture.cleanup();
}
