//! T5.5 whiteout builders: a tree derived from a node at or below a
//! `project.remove_file` frontier records the removed path's absence, while a
//! post-frontier re-creation stays visible. Covers a one-shot Commit, a
//! Workspace Commit bound before the marker, every one-shot tree creator, and
//! a merge whose sides are hidden, re-created, or both.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::{paths, write, Change, Fixture};

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

/// The text of a present file.
fn visible(fixture: &Fixture, branch: &str, path: &str) -> String {
    let value =
        read(fixture, branch, path).unwrap_or_else(|refusal| panic!("{branch}{path}: {refusal:?}"));
    value
        .get_text("text")
        .unwrap_or_else(|| panic!("{branch}{path} has no text: {value:?}"))
        .to_owned()
}

/// A path the committed tree does not hold: above the frontier the read is
/// absent, not `Removed`.
fn absent(fixture: &Fixture, branch: &str, path: &str) {
    let refusal =
        read(fixture, branch, path).expect_err(&format!("{branch}{path} should be absent"));
    assert_eq!(refusal.code, "NoSuchPath", "{branch}{path}: {refusal:?}");
}

#[test]
fn a_one_shot_commit_after_remove_stays_absent() {
    let fixture = Fixture::new("t55-commit", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("work");
    fixture.apply("work", vec![write("/x", "x\n")]);

    remove(&fixture, "/a");

    // A change elsewhere commits a derived tree: the marker's path is absent
    // at the new node, not merely hidden by the read filter (the new node is
    // above the frontier).
    fixture.apply("work", vec![write("/y", "y\n")]);
    absent(&fixture, "work", "/a");
    let listed = fixture.call(
        "fs.list",
        vec![("branch", text("work")), ("path", text("/"))],
    );
    let names = paths(&listed);
    assert!(!names.contains(&"/a"), "{names:?}");
    assert!(names.contains(&"/b"), "{names:?}");
    assert!(names.contains(&"/x") && names.contains(&"/y"), "{names:?}");
    fixture.cleanup();
}

#[test]
fn a_workspace_commit_bound_before_the_marker_materializes_it() {
    // Bound first, then the marker is written before the Commit: the mount
    // saw no whiteout, so the Commit path itself must remove the inherited
    // entry rather than silently resurrect it.
    let fixture = Fixture::new("t55-workspace", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        1
    });
    fixture.fork("work");
    fixture.workspace_commit("work", &[Change::Create("/w", b"w")], 31, || {
        remove(&fixture, "/a");
    });
    absent(&fixture, "work", "/a");
    assert_eq!(visible(&fixture, "work", "/w"), "w");
    let listed = fixture.call(
        "fs.list",
        vec![("branch", text("work")), ("path", text("/"))],
    );
    assert!(!paths(&listed).contains(&"/a"));
    fixture.cleanup();
}

#[test]
fn a_workspace_commit_keeps_a_re_creation_written_before_the_marker() {
    // The workspace rewrote the inherited file, then the marker was written,
    // then the Commit: the rewrite is a re-creation and stays visible.
    let fixture = Fixture::new("t55-workspace-recreate", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        1
    });
    fixture.fork("work");
    fixture.workspace_commit("work", &[Change::Write("/a", b"again\n")], 32, || {
        remove(&fixture, "/a");
    });
    assert_eq!(visible(&fixture, "work", "/a"), "again\n");
    fixture.cleanup();
}

#[test]
fn one_shot_creators_see_a_removed_path_as_absent_and_recreate_it_visibly() {
    let fixture = Fixture::new("t55-one-shot", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    for branch in ["rm", "mvfrom", "mvto", "cpfrom", "cpto", "write", "edit"] {
        fixture.fork(branch);
    }
    remove(&fixture, "/a");

    // Removing or moving from a hidden path is the absence the tree has.
    let refusal = fixture
        .try_call("fs.rm", vec![("branch", text("rm")), ("path", text("/a"))])
        .expect_err("hidden rm");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "fs.mv",
            vec![
                ("branch", text("mvfrom")),
                ("from", text("/a")),
                ("to", text("/m")),
            ],
        )
        .expect_err("hidden mv source");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "fs.cp",
            vec![
                ("branch", text("cpfrom")),
                ("from", text("/a")),
                ("to", text("/c")),
            ],
        )
        .expect_err("hidden cp source");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "fs.edit",
            vec![
                ("branch", text("edit")),
                ("path", text("/a")),
                ("oldText", text("a")),
                ("newText", text("z")),
            ],
        )
        .expect_err("hidden edit");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");

    // Writing, moving or copying to the path, or writing it, is a visible
    // re-creation.
    fixture.call(
        "fs.mv",
        vec![
            ("branch", text("mvto")),
            ("from", text("/b")),
            ("to", text("/a")),
        ],
    );
    assert_eq!(visible(&fixture, "mvto", "/a"), "b\n");
    absent(&fixture, "mvto", "/b");

    fixture.call(
        "fs.cp",
        vec![
            ("branch", text("cpto")),
            ("from", text("/b")),
            ("to", text("/a")),
        ],
    );
    assert_eq!(visible(&fixture, "cpto", "/a"), "b\n");
    assert_eq!(visible(&fixture, "cpto", "/b"), "b\n");

    fixture.call(
        "fs.write",
        vec![
            ("branch", text("write")),
            ("path", text("/a")),
            ("content", text("again\n")),
        ],
    );
    assert_eq!(visible(&fixture, "write", "/a"), "again\n");
    fixture.cleanup();
}

#[test]
fn a_merge_of_two_hidden_sides_keeps_the_path_absent() {
    let fixture = Fixture::new("t55-merge-hidden", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("left");
    fixture.fork("right");
    fixture.apply("left", vec![write("/l", "l\n")]);
    fixture.apply("right", vec![write("/r", "r\n")]);
    remove(&fixture, "/a");

    // Both heads are at or below the frontier. A three-way merge on the raw
    // roots would inherit /a from the base and, being above the frontier,
    // show it again; the derived sides keep it absent.
    fixture.call(
        "branch.merge",
        vec![
            ("branch", text("left")),
            ("with", text("right")),
            ("name", text("merged")),
        ],
    );
    absent(&fixture, "merged", "/a");
    assert_eq!(visible(&fixture, "merged", "/l"), "l\n");
    assert_eq!(visible(&fixture, "merged", "/r"), "r\n");

    // Deterministic in either order.
    fixture.call(
        "branch.merge",
        vec![
            ("branch", text("right")),
            ("with", text("left")),
            ("name", text("merged-reverse")),
        ],
    );
    absent(&fixture, "merged-reverse", "/a");
    fixture.cleanup();
}

#[test]
fn a_merge_keeps_a_post_frontier_re_creation() {
    let fixture = Fixture::new("t55-merge-recreate", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        1
    });
    // `old` stays at the base node (below the frontier); `new` commits a
    // re-creation after the marker, above it.
    fixture.fork("old");
    fixture.fork("new");
    fixture.apply("old", vec![write("/o", "o\n")]);
    remove(&fixture, "/a");
    fixture.call(
        "fs.write",
        vec![
            ("branch", text("new")),
            ("path", text("/a")),
            ("content", text("new\n")),
        ],
    );
    assert_eq!(visible(&fixture, "new", "/a"), "new\n");

    fixture.call(
        "branch.merge",
        vec![
            ("branch", text("old")),
            ("with", text("new")),
            ("name", text("merged")),
        ],
    );
    assert_eq!(visible(&fixture, "merged", "/a"), "new\n");

    // The re-creation wins in the other order too.
    fixture.call(
        "branch.merge",
        vec![
            ("branch", text("new")),
            ("with", text("old")),
            ("name", text("merged-reverse")),
        ],
    );
    assert_eq!(visible(&fixture, "merged-reverse", "/a"), "new\n");
    fixture.cleanup();
}

#[test]
fn a_removed_directory_is_absent_and_can_be_recreated() {
    let fixture = Fixture::new("t55-directory", |source| {
        fs::create_dir(source.join("d")).unwrap();
        fs::write(source.join("d/f"), "f\n").unwrap();
        fs::write(source.join("a"), "a\n").unwrap();
        3
    });
    fixture.fork("rm");
    fixture.fork("mkdir");
    remove(&fixture, "/d");

    let refusal = fixture
        .try_call("fs.rm", vec![("branch", text("rm")), ("path", text("/d"))])
        .expect_err("hidden directory rm");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");

    fixture.call(
        "fs.mkdir",
        vec![("branch", text("mkdir")), ("path", text("/d"))],
    );
    let stat = fixture.call(
        "fs.stat",
        vec![("branch", text("mkdir")), ("path", text("/d"))],
    );
    assert_eq!(stat.get_text("type"), Some("dir"), "{stat:?}");
    absent(&fixture, "mkdir", "/d/f");
    fixture.cleanup();
}
