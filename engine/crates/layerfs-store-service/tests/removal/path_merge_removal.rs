//! T12.3c: `project.remove_file` in derived trees (#52 §12 rows AC-11..AC-14
//! and AC-22). A merge of at-or-below heads materializes the whiteout, a
//! post-frontier re-creation wins, a fast-forward merge creates no tree, a
//! publish of an at-or-below Commit yields a hidden Layer whose mount
//! whiteouts, an unmerged Branch that existed before the frontier hides but
//! does not inherit, and a merge started together with the removal never
//! leaves the path visible.
#[path = "../support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store_service::lookup;
use std::fs;
use support::{write, Change, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn remove(fixture: &Fixture, path: &str) -> SdkValue {
    fixture.call("project.remove_file", vec![("path", text(path))])
}

fn try_remove(fixture: &Fixture, path: &str) -> Result<SdkValue, SdkRefusal> {
    fixture.try_call("project.remove_file", vec![("path", text(path))])
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

/// The path is hidden by the marker at this node: the read refuses `Removed`
/// and names the frontier.
fn hidden(fixture: &Fixture, branch: &str, path: &str) {
    let refusal =
        read(fixture, branch, path).expect_err(&format!("{branch}{path} should be hidden"));
    assert_eq!(refusal.code, "Removed", "{branch}{path}: {refusal:?}");
    let paths = match refusal.detail.get("paths") {
        Some(SdkValue::List(paths)) => paths,
        other => panic!("Removed detail without paths: {other:?}"),
    };
    assert_eq!(paths.len(), 1, "{paths:?}");
    assert_eq!(paths[0].get_text("path"), Some(path), "{paths:?}");
    assert!(paths[0].get_int("frontier_serial").is_some(), "{paths:?}");
}

/// The path is absent from the committed tree: above the frontier the read
/// is `NoSuchPath`, never `Removed`.
fn absent(fixture: &Fixture, branch: &str, path: &str) {
    let refusal =
        read(fixture, branch, path).expect_err(&format!("{branch}{path} should be absent"));
    assert_eq!(refusal.code, "NoSuchPath", "{branch}{path}: {refusal:?}");
}

/// Rows of one history table in the installed Store.
fn rows(fixture: &Fixture, table: &str) -> i64 {
    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    connection
        .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn merge(fixture: &Fixture, left: &str, right: &str, name: &str) -> Result<SdkValue, SdkRefusal> {
    fixture.try_call(
        "branch.merge",
        vec![
            ("branch", text(left)),
            ("with", text(right)),
            ("name", text(name)),
        ],
    )
}

#[test]
fn a_fast_forward_merge_creates_no_tree_and_the_path_stays_hidden() {
    let fixture = Fixture::new("t123c-ff", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("left");
    fixture.apply("left", vec![write("/l", "l\n")]);
    // `fast` starts at left's Commit and commits one child: left contains no
    // `fast` node, while `fast` contains every `left` node.
    fixture.call(
        "branch.fork",
        vec![("name", text("fast")), ("from", text("left"))],
    );
    fixture.apply("fast", vec![write("/r", "r\n")]);
    remove(&fixture, "/a");
    let (commits, layers) = (rows(&fixture, "commit"), rows(&fixture, "layer"));

    // One side contains the other: a fast-forward Branch at the newer node.
    let merged = merge(&fixture, "left", "fast", "merged").expect("a fast-forward merge");
    assert!(merged.get_bool("fastForward"), "{merged:?}");
    assert_eq!(
        rows(&fixture, "commit"),
        commits,
        "a fast-forward merge writes no new Commit"
    );
    assert_eq!(rows(&fixture, "layer"), layers, "and no new Layer");

    // The node it points at is at or below the frontier: the marker hides it
    // without any whiteout, and the other files stay visible.
    hidden(&fixture, "merged", "/a");
    assert_eq!(visible(&fixture, "merged", "/l"), "l\n");
    assert_eq!(visible(&fixture, "merged", "/r"), "r\n");
    fixture.cleanup();
}

#[test]
fn publishing_an_at_or_below_commit_yields_a_hidden_layer_and_a_layer_headed_mount() {
    let fixture = Fixture::new("t123c-publish", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    fixture.fork("work");
    fixture.apply("work", vec![write("/w", "w\n")]);
    remove(&fixture, "/a");

    // `work`'s Commit is at or below the frontier: publishing republishes its
    // tree as a Layer at the same position, so the marker hides it on main.
    let published = fixture.call("branch.publish", vec![("branch", text("work"))]);
    assert_eq!(published.get_text("kind"), Some("Added"), "{published:?}");
    hidden(&fixture, "main", "/a");
    assert_eq!(visible(&fixture, "main", "/w"), "w\n");

    // A Branch forked from main is layer-headed (no head Commit) and its base
    // Layer is the published, at-or-below one: the mount whiteouts /a, and the
    // next Commit records the deletion rather than inheriting it.
    fixture.fork("layer");
    fixture.workspace_commit("layer", &[Change::Create("/n", b"n")], 41, || {});
    absent(&fixture, "layer", "/a");
    assert_eq!(visible(&fixture, "layer", "/n"), "n");
    assert_eq!(visible(&fixture, "layer", "/b"), "b\n");
    fixture.cleanup();
}

#[test]
fn an_unmerged_branch_created_before_the_frontier_hides_but_does_not_inherit() {
    let fixture = Fixture::new("t123c-unmerged", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    // The Branch and its Commit exist before the marker.
    fixture.fork("old");
    fixture.apply("old", vec![write("/x", "x\n")]);
    remove(&fixture, "/a");

    // The earlier Commit's path is hidden by the marker.
    hidden(&fixture, "old", "/a");
    assert_eq!(visible(&fixture, "old", "/x"), "x\n");

    // A Commit after the frontier on that unmerged Branch does not inherit
    // it: the derived tree records the absence.
    fixture.apply("old", vec![write("/y", "y\n")]);
    absent(&fixture, "old", "/a");
    assert_eq!(visible(&fixture, "old", "/x"), "x\n");
    assert_eq!(visible(&fixture, "old", "/y"), "y\n");
    fixture.cleanup();
}

#[test]
fn a_recreated_removed_path_still_conflicts_as_an_ordinary_edit() {
    let fixture = Fixture::new("t123c-recreate-conflict", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        1
    });
    // `side` stays at the base node (at or below the frontier).
    fixture.fork("side");
    fixture.apply("side", vec![write("/s", "s\n")]);
    remove(&fixture, "/a");

    // Two Branches re-create /a after the frontier; both are above it, so the
    // marker no longer applies and this is an ordinary edit conflict.
    fixture.fork("one");
    fixture.apply("one", vec![write("/a", "one\n")]);
    fixture.fork("two");
    fixture.apply("two", vec![write("/a", "two\n")]);
    assert_eq!(visible(&fixture, "one", "/a"), "one\n");
    assert_eq!(visible(&fixture, "two", "/a"), "two\n");

    let refusal =
        merge(&fixture, "one", "two", "conflicted").expect_err("an ordinary edit conflict");
    assert_eq!(refusal.code, "Conflict", "{refusal:?}");
    let conflicts = match refusal.detail.get("conflicts") {
        Some(SdkValue::List(conflicts)) => conflicts,
        other => panic!("Conflict detail without conflicts: {other:?}"),
    };
    let paths: Vec<&str> = conflicts
        .iter()
        .map(|conflict| conflict.get_text("path").unwrap_or(""))
        .collect();
    assert_eq!(paths, ["/a"], "{conflicts:?}");
    // A conflicted merge creates nothing.
    let missing = fixture
        .try_call(
            "fs.read",
            vec![("branch", text("conflicted")), ("path", text("/a"))],
        )
        .expect_err("the conflicted merge created no Branch");
    assert_eq!(missing.code, "NoSuchBranch", "{missing:?}");
    fixture.cleanup();
}

#[test]
fn a_merge_that_cannot_materialize_a_whiteout_is_refused_removed() {
    let fixture = Fixture::new("t123c-unmaterializable", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        1
    });
    fixture.fork("left");
    fixture.apply("left", vec![write("/l", "l\n")]);
    fixture.fork("right");
    fixture.apply("right", vec![write("/r", "r\n")]);

    // A marker whose path no tree can name (a legacy or corrupted marker):
    // the sides cannot whiteout it, so the merge refuses `Removed` with the
    // full element instead of building a tree that leaves it visible.
    let stack = lookup::project(&fixture.service.store, "installed").expect("installed project");
    fixture
        .service
        .store
        .history()
        .mark_path_removed(stack.id, b"../bad", 1_000_000, 7)
        .expect("write the malformed marker");
    let refusal = merge(&fixture, "left", "right", "merged")
        .expect_err("the merge cannot materialize the whiteout");
    assert_eq!(refusal.code, "Removed", "{refusal:?}");
    let paths = match refusal.detail.get("paths") {
        Some(SdkValue::List(paths)) => paths,
        other => panic!("Removed detail without paths: {other:?}"),
    };
    assert_eq!(paths.len(), 1, "{paths:?}");
    assert_eq!(paths[0].get_text("path"), Some("../bad"), "{paths:?}");
    assert_eq!(paths[0].get_int("frontier_serial"), Some(1_000_000));
    // Nothing is created for a refused merge.
    let missing = fixture
        .try_call(
            "fs.read",
            vec![("branch", text("merged")), ("path", text("/a"))],
        )
        .expect_err("the refused merge created no Branch");
    assert_eq!(missing.code, "NoSuchBranch", "{missing:?}");
    fixture.cleanup();
}

#[test]
fn a_merge_and_a_removal_started_together_never_leave_the_path_visible() {
    let fixture = Fixture::new("t123c-race", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("b"), "b\n").unwrap();
        2
    });
    for round in 0..3u8 {
        let left = format!("left-{round}");
        let right = format!("right-{round}");
        let merged = format!("merged-{round}");
        fixture.fork(&left);
        fixture.fork(&right);
        fixture.apply(&left, vec![write(&format!("/l-{round}"), "l\n")]);
        fixture.apply(&right, vec![write(&format!("/r-{round}"), "r\n")]);

        // A deterministic barrier starts the merge and the removal together.
        let barrier = std::sync::Barrier::new(2);
        let (merge_result, remove_result) = std::thread::scope(|scope| {
            let merge_barrier = &barrier;
            let remove_barrier = &barrier;
            let merging = scope.spawn(|| {
                merge_barrier.wait();
                merge(&fixture, &left, &right, &merged)
            });
            let removing = scope.spawn(|| {
                remove_barrier.wait();
                try_remove(&fixture, "/a")
            });
            (merging.join().unwrap(), removing.join().unwrap())
        });

        // The loser retries: a removal refused by the live merge (or its
        // operation) is repeated after it, and a merge refused by the epoch
        // fence is repeated with the marker in place.
        if let Err(refusal) = &remove_result {
            assert!(
                matches!(
                    refusal.code.as_str(),
                    "MountsActive" | "Busy" | "Unresolved"
                ),
                "round {round}: {refusal:?}"
            );
            remove(&fixture, "/a");
        }
        if let Err(refusal) = &merge_result {
            assert_eq!(refusal.code, "Conflict", "round {round}: {refusal:?}");
            merge(&fixture, &left, &right, &merged).expect("the retried merge");
        }

        // Neither order may leave the removed path visible on the merge.
        match read(&fixture, &merged, "/a") {
            Ok(value) => panic!("round {round}: /a is visible: {value:?}"),
            Err(refusal) => assert!(
                matches!(refusal.code.as_str(), "Removed" | "NoSuchPath"),
                "round {round}: {refusal:?}"
            ),
        }
        // A tree derived above the frontier records the absence.
        fixture.apply(&merged, vec![write(&format!("/z-{round}"), "z\n")]);
        absent(&fixture, &merged, "/a");
        assert_eq!(visible(&fixture, &merged, &format!("/l-{round}")), "l\n");
        assert_eq!(visible(&fixture, &merged, &format!("/r-{round}")), "r\n");
    }
    fixture.cleanup();
}
