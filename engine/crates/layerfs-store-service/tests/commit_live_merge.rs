//! A Commit that merges installs a root other than the Workspace's own.
//! Rows written after the capture were computed against the old base, so an
//! install under rows that overlap what the merge changed is refused
//! (`LiveConflict`) before anything is published: the capture folds back,
//! nothing is lost, and the next Commit takes everything. A row carried only
//! for an mtime-only change holds no data: the merged install drops it when
//! its serial changed. Mutations that arrive between the check and the
//! install wait for the install.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/live.rs"]
#[allow(dead_code)]
mod live;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_history::CommitOutcome;
use layerfs_store_service::store::{
    BoundWorkspace, CommitError, CommitFailure, CommitOptions, CommitSuccess,
};
use layerfs_store_service::telemetry::{mark_phases, Recording};
use std::{cell::Cell, rc::Rc, sync::Once};
use support::Change;

fn tmp() {
    static SET: Once = Once::new();
    SET.call_once(|| std::env::set_var("TMPDIR", env!("CARGO_TARGET_TMPDIR")));
}

fn base(source: &std::path::Path) -> usize {
    std::fs::write(source.join("a.txt"), b"alpha\n").unwrap();
    std::fs::write(source.join("b.txt"), b"bravo\n").unwrap();
    std::fs::write(source.join("f.txt"), b"0123456789\n").unwrap();
    std::fs::create_dir(source.join("d")).unwrap();
    std::fs::write(source.join("d/x"), b"x\n").unwrap();
    5
}

fn options() -> CommitOptions {
    CommitOptions {
        force: false,
        message: Some("live merge".into()),
        expected: None,
    }
}

/// Commits `workspace`; `during` runs once on the Commit thread at the start
/// of the phase `at` (after the capture), as a write in the Commit window.
fn commit_with_window(
    workspace: &BoundWorkspace,
    at: &'static str,
    during: impl FnOnce() + 'static,
) -> Result<CommitSuccess, Box<CommitFailure>> {
    let pending = Rc::new(Cell::new(Some(Box::new(during) as Box<dyn FnOnce()>)));
    let take = pending.clone();
    let recording = Recording::start();
    mark_phases(Some(Box::new(move |name, end| {
        if name == at && !end {
            if let Some(during) = take.take() {
                during();
            }
        }
    })));
    let result = workspace.commit_captured(&options());
    mark_phases(None);
    drop(recording);
    assert!(pending.take().is_none(), "the window at {at} was reached");
    result
}

/// A `'static` handle on a Workspace for a window closure. SAFETY: every
/// closure runs inside the Commit of that same Workspace, which outlives it.
fn handle(workspace: &BoundWorkspace) -> &'static BoundWorkspace {
    unsafe { &*(workspace as *const BoundWorkspace) }
}

fn live_conflict(failure: &CommitFailure) -> &[String] {
    match &failure.error {
        CommitError::LiveConflict { paths } => paths,
        other => panic!("not a live conflict: {other:?} ({failure:?})"),
    }
}

fn text(workspace: &BoundWorkspace, path: &str) -> String {
    String::from_utf8(live::live(workspace, path).unwrap()).unwrap()
}

/// `touch F`; the other side commits a longer F; this side's Commit merges
/// and installs it. F must read whole, not cut at its old length, and the
/// next Commit has nothing to publish.
#[test]
fn touched_file_is_not_truncated_by_a_merge() {
    tmp();
    let fixture = support::Fixture::new("live-touch", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 81, "lane");
    live::change(&workspace, &[Change::Mtime("/f.txt", 999)]);
    let longer = b"0123456789 and twenty more bytes\n";
    fixture.workspace_commit("lane", &[Change::Write("/f.txt", longer)], 82, || {});
    let committed = workspace
        .commit_captured(&options())
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert!(committed.merge.is_some(), "merged");
    assert_eq!(live::live(&workspace, "/f.txt").unwrap(), longer);
    let again = workspace.commit_captured(&options()).unwrap();
    assert!(
        matches!(again.history, Some(CommitOutcome::UpToDate { .. })),
        "{:?}",
        again.history
    );
    assert_eq!(
        live::committed(&fixture, "lane", "/f.txt").as_deref(),
        Some(std::str::from_utf8(longer).unwrap())
    );
    workspace.close().unwrap();
    fixture.cleanup();
}

/// `rm d/x` after the capture while the merge adds `d/y`: refused, nothing
/// lost; `d` is not empty and the next Commit keeps `y` and removes `x`.
#[test]
fn removal_after_capture_under_a_merged_directory_is_refused() {
    tmp();
    let fixture = support::Fixture::new("live-rm", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 83, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"ALPHA\n")]);
    fixture.workspace_commit("lane", &[Change::Create("/d/y", b"y\n")], 84, || {});
    let window = handle(&workspace);
    let refused = commit_with_window(&workspace, "merge", move || {
        live::change(window, &[Change::Unlink("/d/x")]);
    })
    .expect_err("live conflict");
    assert!(
        live_conflict(&refused).contains(&"/d".to_owned()),
        "{refused:?}"
    );
    assert!(refused.locally_settled, "{refused:?}");
    assert!(refused.published.is_none(), "{refused:?}");
    assert_eq!(text(&workspace, "/a.txt"), "ALPHA\n");
    assert!(live::live(&workspace, "/d/x").is_err(), "x stays removed");
    let committed = workspace
        .commit_captured(&options())
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert!(committed.merge.is_some(), "merged");
    assert_eq!(text(&workspace, "/d/y"), "y\n");
    assert!(live::live(&workspace, "/d/x").is_err());
    assert!(
        live::try_change(&workspace, Change::Rmdir("/d")).is_err(),
        "d holds y"
    );
    assert_eq!(live::committed(&fixture, "lane", "/d/x"), None);
    assert_eq!(
        live::committed(&fixture, "lane", "/d/y").as_deref(),
        Some("y\n")
    );
    assert_eq!(
        live::committed(&fixture, "lane", "/a.txt").as_deref(),
        Some("ALPHA\n")
    );
    workspace.close().unwrap();
    fixture.cleanup();
}

/// A write during the Commit window to a file the other side changed is
/// refused and kept.
#[test]
fn write_in_the_window_to_a_merged_file_is_refused() {
    tmp();
    let fixture = support::Fixture::new("live-write", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 85, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"ALPHA\n")]);
    fixture.workspace_commit("lane", &[Change::Write("/b.txt", b"BRAVO\n")], 86, || {});
    let window = handle(&workspace);
    let refused = commit_with_window(&workspace, "lane", move || {
        live::change(window, &[Change::Write("/b.txt", b"bravo, mine\n")]);
    })
    .expect_err("live conflict");
    assert_eq!(live_conflict(&refused), ["/b.txt".to_owned()]);
    assert!(refused.locally_settled, "{refused:?}");
    assert_eq!(text(&workspace, "/b.txt"), "bravo, mine\n");
    assert_eq!(text(&workspace, "/a.txt"), "ALPHA\n");
    workspace.close().unwrap();
    fixture.cleanup();
}

/// Both sides create directory `n`: the merge keeps the other side's `n`,
/// so this side's directory serial is gone from the merged root. A file
/// created under this side's `n` after the capture is refused, and the next
/// Commit merges all three files into one `n`.
#[test]
fn rows_under_a_directory_the_merge_replaced_are_refused() {
    tmp();
    let fixture = support::Fixture::new("live-dir", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 87, "lane");
    live::change(
        &workspace,
        &[Change::Mkdir("/n"), Change::Create("/n/a", b"a\n")],
    );
    fixture.workspace_commit(
        "lane",
        &[Change::Mkdir("/n"), Change::Create("/n/b", b"b\n")],
        88,
        || {},
    );
    let window = handle(&workspace);
    let refused = commit_with_window(&workspace, "merge", move || {
        live::change(window, &[Change::Create("/n/c", b"c\n")]);
    })
    .expect_err("live conflict");
    assert!(
        live_conflict(&refused).contains(&"/n".to_owned()),
        "{refused:?}"
    );
    assert!(refused.locally_settled, "{refused:?}");
    workspace
        .commit_captured(&options())
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    for (path, bytes) in [("/n/a", "a\n"), ("/n/b", "b\n"), ("/n/c", "c\n")] {
        assert_eq!(text(&workspace, path), bytes, "{path}");
        assert_eq!(
            live::committed(&fixture, "lane", path).as_deref(),
            Some(bytes),
            "{path}"
        );
    }
    workspace.close().unwrap();
    fixture.cleanup();
}

/// A merge whose changes no row written after the capture touches still
/// publishes and installs; the new file stays on top of the merged view.
#[test]
fn a_merge_with_no_overlap_publishes_and_installs() {
    tmp();
    let fixture = support::Fixture::new("live-clean", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 89, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"ALPHA\n")]);
    fixture.workspace_commit("lane", &[Change::Write("/b.txt", b"BRAVO\n")], 90, || {});
    let window = handle(&workspace);
    let committed = commit_with_window(&workspace, "merge", move || {
        live::change(window, &[Change::Create("/c.txt", b"charlie\n")]);
    })
    .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert!(
        matches!(committed.history, Some(CommitOutcome::Committed { .. })),
        "{:?}",
        committed.history
    );
    assert!(committed.replaced.is_some(), "a merged root installed");
    assert_eq!(text(&workspace, "/a.txt"), "ALPHA\n");
    assert_eq!(text(&workspace, "/b.txt"), "BRAVO\n");
    assert_eq!(text(&workspace, "/c.txt"), "charlie\n");
    assert_eq!(live::committed(&fixture, "lane", "/c.txt"), None);
    workspace
        .commit_captured(&options())
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        live::committed(&fixture, "lane", "/c.txt").as_deref(),
        Some("charlie\n")
    );
    workspace.close().unwrap();
    fixture.cleanup();
}

/// The hold a merged Commit keeps from its check of the active rows through
/// its install: a mutation that arrives while it is held waits, lock
/// released, and runs once it is dropped. Reads and other jobs run.
///
/// The Commit itself is not driven here: this harness mutates through a
/// counted base source, and a known install waits for base sources to
/// drain, so a write held during a Commit's publication would never let
/// that install run. No product caller mutates while holding one (kernel
/// requests take none; SDK file writes release theirs before their Commit).
#[test]
fn a_held_workspace_delays_mutations_until_the_hold_ends() {
    tmp();
    let fixture = support::Fixture::new("live-hold", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 91, "lane");
    let route = workspace.operation().unwrap().workspace().route();
    let hold = fixture.owner.client().hold_mutations(route).unwrap();
    let landed = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let (workspace, landed) = (&workspace, &landed);
        let writer = scope.spawn(move || {
            live::change(workspace, &[Change::Write("/b.txt", b"BRAVO\n")]);
            landed.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!landed.load(std::sync::atomic::Ordering::SeqCst), "held");
        assert_eq!(text(workspace, "/b.txt"), "bravo\n", "reads run");
        drop(hold);
        writer.join().unwrap();
    });
    assert_eq!(text(&workspace, "/b.txt"), "BRAVO\n");
    workspace.close().unwrap();
    fixture.cleanup();
}
