//! How a failed Commit settles. Before the Publish phase nothing in history
//! can have changed, so a Store, Content or History failure there is a
//! definite nonpublication: the capture folds back and the Workspace takes
//! the next Commit. Only local uncertainty (the overlay, an owner job, the
//! binding) keeps custody. A conflicted merge publishes nothing and takes no
//! publication lane.
#[path = "../support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../support/live.rs"]
#[allow(dead_code)]
mod live;
#[path = "../support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_content::ContentError;
use layerfs_history::CommitOutcome;
use layerfs_store_service::store::{CommitError, CommitOptions, CommitPhase};
use layerfs_store_service::telemetry::{mark_phases, Recording};
use layerfs_vfs::WorkspaceError;
use std::{cell::RefCell, rc::Rc, sync::Once};
use support::Change;

fn tmp() {
    static SET: Once = Once::new();
    SET.call_once(|| std::env::set_var("TMPDIR", env!("CARGO_TARGET_TMPDIR")));
}

fn base(source: &std::path::Path) -> usize {
    std::fs::write(source.join("a.txt"), b"alpha\n").unwrap();
    std::fs::write(source.join("b.txt"), b"bravo\n").unwrap();
    2
}

fn options() -> CommitOptions {
    CommitOptions {
        force: false,
        message: Some("settle".into()),
        expected: None,
    }
}

/// A read failure after construction, of the class merge, count and hint
/// reads report (`Content` with no provider cause), is definite before the
/// Publish phase: the capture folds back, nothing is lost, and the next
/// Commit publishes the same change.
#[test]
fn content_failure_before_publication_folds_back() {
    tmp();
    let fixture = support::Fixture::new("settle-content", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 51, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"ALPHA\n")]);
    let failure = workspace
        .commit(|_, _, _| {
            Err(CommitError::Content {
                error: ContentError::OutputRejected,
                provider: None,
            })
        })
        .expect_err("refused construction");
    assert_eq!(failure.phase, CommitPhase::Construct, "{failure:?}");
    assert!(
        failure.error.uncertain(),
        "the class alone reads as unknown"
    );
    assert!(!failure.uncertain(), "nothing could publish: {failure:?}");
    assert!(failure.locally_settled, "capture folded back: {failure:?}");
    assert_eq!(live::live(&workspace, "/a.txt").unwrap(), b"ALPHA\n");
    let committed = workspace.commit_captured(&options()).unwrap();
    assert!(
        matches!(committed.history, Some(CommitOutcome::Committed { .. })),
        "{:?}",
        committed.history
    );
    assert_eq!(
        live::committed(&fixture, "lane", "/a.txt").as_deref(),
        Some("ALPHA\n")
    );
    workspace.close().unwrap();
    fixture.cleanup();
}

/// Local uncertainty keeps custody whatever the phase.
#[test]
fn local_uncertainty_keeps_custody() {
    tmp();
    let fixture = support::Fixture::new("settle-local", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 52, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"ALPHA\n")]);
    let failure = workspace
        .commit(|_, _, _| Err(CommitError::Workspace(WorkspaceError::BindingPoisoned)))
        .expect_err("refused construction");
    assert!(failure.uncertain(), "{failure:?}");
    assert!(!failure.locally_settled, "custody kept: {failure:?}");
    let next = workspace
        .commit_captured(&options())
        .expect_err("admission still held");
    assert!(matches!(next.error, CommitError::InFlight), "{next:?}");
    fixture.cleanup();
}

/// A conflicted merge publishes nothing and never takes the lane; the
/// merge with markers is installed and nothing is lost.
#[test]
fn conflicted_merge_takes_no_lane() {
    tmp();
    let fixture = support::Fixture::new("settle-conflict-lane", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 53, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"ours\n")]);
    fixture.workspace_commit("lane", &[Change::Write("/a.txt", b"theirs\n")], 54, || {});
    let phases = Rc::new(RefCell::new(Vec::new()));
    let seen = phases.clone();
    let recording = Recording::start();
    mark_phases(Some(Box::new(move |name, end| {
        if !end {
            seen.borrow_mut().push(name);
        }
    })));
    let committed = workspace.commit_captured(&options());
    mark_phases(None);
    drop(recording);
    let committed = committed.unwrap_or_else(|failure| panic!("{failure:?}"));
    assert!(committed.history.is_none(), "nothing published");
    let merge = committed.merge.expect("merged");
    assert_eq!(merge.conflicts.len(), 1, "{merge:?}");
    let phases = phases.borrow();
    assert!(phases.contains(&"merge"), "{phases:?}");
    assert!(!phases.contains(&"lane"), "lane taken: {phases:?}");
    assert!(!phases.contains(&"publish"), "{phases:?}");
    let shown = String::from_utf8(live::live(&workspace, "/a.txt").unwrap()).unwrap();
    assert!(
        shown.contains("ours") && shown.contains("theirs"),
        "{shown}"
    );
    workspace.close().unwrap();
    fixture.cleanup();
}
