//! T5.5 whiteout builders: a tree derived from a node at or below a
//! `project.hide_path` frontier records the removed path's absence, while a
//! post-frontier re-creation stays visible. Covers a one-shot Commit, a
//! Workspace Commit bound before the marker, every one-shot tree creator, and
//! a merge whose sides are hidden, re-created, or both.
use crate::support;

use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{BranchId, CommitOutcome, HistoryName, LayerStackId, WorkspaceId};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store_service::filesystem::change::mutation::Mutation;
use layerfs_store_service::store::{BindRequest, CommitOptions};
use std::fs;
use support::{paths, write, Change, Fixture};

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
        "file.list",
        vec![("branch", text("work")), ("path", text("/"))],
    );
    let names = paths(&listed);
    assert!(!names.contains(&"/a"), "{names:?}");
    assert!(names.contains(&"/b"), "{names:?}");
    assert!(names.contains(&"/x") && names.contains(&"/y"), "{names:?}");
    fixture.cleanup();
}

#[test]
fn a_marker_under_a_live_mount_is_refused_and_materializes_after_it_closes() {
    // Admission refuses  while any mount intent holds the
    // project, so a marker can never appear under a live Workspace. The Commit
    // path still has to materialize a marker written before the Workspace
    // binds, which the second Workspace below commits through.
    let fixture = Fixture::new("t55-workspace", |source| {
        fs::write(
            source.join("a"),
            "a
",
        )
        .unwrap();
        1
    });
    fixture.fork("work");
    fixture.workspace_commit("work", &[Change::Create("/w", b"w")], 31, || {
        let refusal = fixture
            .try_call("project.hide_path", vec![("path", text("/a"))])
            .expect_err("a marker under a live mount");
        assert_eq!(refusal.code, "MountsActive", "{refusal:?}");
        let holders = match refusal.detail.get("holders") {
            Some(SdkValue::List(holders)) => holders,
            other => panic!("MountsActive detail without holders: {other:?}"),
        };
        assert_eq!(holders.len(), 1, "{holders:?}");
        assert_eq!(holders[0].get_text("kind"), Some("mount"), "{holders:?}");
        assert_eq!(
            holders[0].get_text("workspace"),
            Some("1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f"),
            "{holders:?}"
        );
    });
    remove(&fixture, "/a");
    // A Workspace bound after the marker materializes the whiteout into its
    // overlay: a Commit of a change elsewhere must not resurrect /a.
    fixture.workspace_commit("work", &[Change::Create("/v", b"v")], 33, || {});
    absent(&fixture, "work", "/a");
    assert_eq!(visible(&fixture, "work", "/w"), "w");
    assert_eq!(visible(&fixture, "work", "/v"), "v");
    let listed = fixture.call(
        "file.list",
        vec![("branch", text("work")), ("path", text("/"))],
    );
    let names = paths(&listed);
    assert!(!names.contains(&"/a"), "{names:?}");
    assert!(names.contains(&"/w") && names.contains(&"/v"), "{names:?}");
    fixture.cleanup();
}

#[test]
fn a_workspace_commit_keeps_a_re_creation_above_the_marker() {
    // The marker comes first, as admission requires while no mount is live.
    // The Workspace then binds below the frontier, materializes the whiteout,
    // and writes the path again above it: the re-creation stays visible.
    let fixture = Fixture::new("t55-workspace-recreate", |source| {
        fs::write(
            source.join("a"),
            "a
",
        )
        .unwrap();
        1
    });
    fixture.fork("work");
    remove(&fixture, "/a");
    fixture.workspace_commit(
        "work",
        &[Change::Create(
            "/a", b"again
",
        )],
        32,
        || {},
    );
    assert_eq!(
        visible(&fixture, "work", "/a"),
        "again
"
    );
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

    // Removing or moving from a hidden path is the absence the tree has;
    // edit and a cp source are `Removed` (T5.6, §4.3).
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
                ("to", text("/m")),
            ],
        )
        .expect_err("hidden mv source");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");
    let refusal = fixture
        .try_call(
            "file.cp",
            vec![
                ("branch", text("cpfrom")),
                ("from", text("/a")),
                ("to", text("/c")),
            ],
        )
        .expect_err("hidden cp source");
    assert_eq!(refusal.code, "Removed", "{refusal:?}");
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
    assert_eq!(refusal.code, "Removed", "{refusal:?}");

    // Writing, moving or copying to the path, or writing it, is a visible
    // re-creation.
    fixture.call(
        "file.mv",
        vec![
            ("branch", text("mvto")),
            ("from", text("/b")),
            ("to", text("/a")),
        ],
    );
    assert_eq!(visible(&fixture, "mvto", "/a"), "b\n");
    absent(&fixture, "mvto", "/b");

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

    fixture.call(
        "file.write",
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
        "file.write",
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
        .try_call(
            "file.rm",
            vec![("branch", text("rm")), ("path", text("/d"))],
        )
        .expect_err("hidden directory rm");
    assert_eq!(refusal.code, "NoSuchPath", "{refusal:?}");

    fixture.call(
        "file.mkdir",
        vec![("branch", text("mkdir")), ("path", text("/d"))],
    );
    let stat = fixture.call(
        "file.stat",
        vec![("branch", text("mkdir")), ("path", text("/d"))],
    );
    assert_eq!(stat.get_text("type"), Some("dir"), "{stat:?}");
    absent(&fixture, "mkdir", "/d/f");
    fixture.cleanup();
}

#[test]
fn a_mount_keeps_its_markers_for_every_commit_it_makes() {
    // A Workspace bound below the frontier keeps the markers its mount read
    // and gives them to its Commits; a Commit it published hides nothing
    // (#768). The first Commit's constructor returns the bound root itself,
    // ignoring the overlay's whiteout, so `/a` can only be absent from it if
    // the Commit's own whiteout step removed it with the kept markers. The
    // second Commit, over the Workspace's own Commit, keeps `/a` absent and
    // its new file visible.
    let fixture = Fixture::new("t55-workspace-kept", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("work");
    remove(&fixture, "/a");
    let workspace = fixture
        .service
        .store
        .bind(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([34; 32]).unwrap(),
                branch: BranchId::named(
                    LayerStackId::from_authority([23; 16]),
                    &HistoryName::new("work").unwrap(),
                ),
            },
        )
        .unwrap()
        .workspace;
    let first = workspace
        .commit(|_, _, snapshot| Ok(FilesystemRootId(snapshot.effective_root)))
        .unwrap_or_else(|failure| panic!("first Commit: {failure:?}"));
    assert!(
        matches!(first.history, Some(CommitOutcome::Committed { .. })),
        "{:?}",
        first.history
    );
    absent(&fixture, "work", "/a");
    assert_eq!(visible(&fixture, "work", "/b"), "b\n");
    let mut change = Mutation::new(&workspace).unwrap();
    change.write("/x", b"x\n", None).unwrap();
    change.finish().unwrap();
    let second = workspace
        .commit_captured(&CommitOptions {
            force: false,
            message: Some("over its own Commit".into()),
            expected: None,
        })
        .unwrap_or_else(|failure| panic!("second Commit: {failure:?}"));
    assert!(
        matches!(second.history, Some(CommitOutcome::Committed { .. })),
        "{:?}",
        second.history
    );
    absent(&fixture, "work", "/a");
    assert_eq!(visible(&fixture, "work", "/b"), "b\n");
    assert_eq!(visible(&fixture, "work", "/x"), "x\n");
    workspace.close().unwrap();
    fixture.cleanup();
}
