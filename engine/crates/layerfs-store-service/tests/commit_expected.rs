//! A Workspace Commit made over one head only (`CommitOptions::expected`,
//! `--expected`) and a forced Commit. The expected head is checked when the
//! head is read and again inside the publication's own transaction, so a
//! head that moves after the check, even inside the lane, is refused
//! `HeadMoved` with nothing published. A forced Commit is decided by the
//! head its publication reads in the lane, not by a read before it.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/live.rs"]
#[allow(dead_code)]
mod live;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_history::{CommitId, CommitOutcome, HistoryError, NodeId};
use layerfs_store_service::store::{BoundWorkspace, CommitError, CommitOptions, CommitPhase};
use layerfs_store_service::telemetry::{mark_phases, Recording};
use std::{cell::Cell, sync::Once};
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

fn head(fixture: &support::Fixture) -> Option<CommitId> {
    fixture
        .service
        .store
        .history()
        .branch_snapshot(live::branch("lane"))
        .unwrap()
        .unwrap()
        .branch
        .head_commit
}

/// Another Workspace commits `bytes` to `/b.txt` on the lane.
fn land(fixture: &support::Fixture, id: u8, bytes: &[u8]) -> CommitId {
    fixture.workspace_commit("lane", &[Change::Write("/b.txt", bytes)], id, || {});
    head(fixture).expect("landed")
}

fn options(force: bool, expected: Option<CommitId>) -> CommitOptions {
    CommitOptions {
        force,
        message: Some("expected".into()),
        expected: expected.map(NodeId::Commit),
    }
}

/// Commits `workspace` while, as the lane is about to be taken, another
/// Workspace lands a Commit on the lane: the head moves inside the window
/// a read before the lane cannot see.
fn commit_racing_lane(
    fixture: &support::Fixture,
    workspace: &BoundWorkspace,
    options: &CommitOptions,
    lander: u8,
) -> (
    Result<
        layerfs_store_service::store::CommitSuccess,
        Box<layerfs_store_service::store::CommitFailure>,
    >,
    CommitId,
) {
    let landed = std::rc::Rc::new(Cell::new(None));
    let busy = std::rc::Rc::new(Cell::new(false));
    let set = landed.clone();
    let host = fixture as *const support::Fixture as usize;
    let recording = Recording::start();
    mark_phases(Some(Box::new(move |name, end| {
        // Once, at the first lane of this Commit; the landing Commit's own
        // phases pass through here too and are ignored.
        if name == "lane" && !end && set.get().is_none() && !busy.get() {
            // SAFETY: the fixture outlives this Commit, which runs on this
            // thread; the hook is removed before the fixture is used again.
            let fixture = unsafe { &*(host as *const support::Fixture) };
            busy.set(true);
            // Bytes of its own, so the landing is never UpToDate.
            let bytes = format!("landed by {lander}\n");
            set.set(Some(land(fixture, lander, bytes.as_bytes())));
            busy.set(false);
        }
    })));
    let result = workspace.commit_captured(options);
    mark_phases(None);
    drop(recording);
    (
        result,
        landed.get().expect("a Commit landed before the lane"),
    )
}

#[test]
fn a_moved_expected_head_is_refused_before_the_merge() {
    tmp();
    let fixture = support::Fixture::new("expected-moved", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 61, "lane");
    let bound = head(&fixture);
    live::change(&workspace, &[Change::Write("/a.txt", b"ours!\n")]);
    let theirs = land(&fixture, 62, b"theirs\n");
    let refused = workspace
        .commit_captured(&CommitOptions {
            expected: Some(match bound {
                Some(id) => NodeId::Commit(id),
                None => NodeId::Layer(
                    fixture
                        .service
                        .store
                        .history()
                        .branch_snapshot(live::branch("lane"))
                        .unwrap()
                        .unwrap()
                        .branch
                        .base_layer,
                ),
            }),
            ..options(false, None)
        })
        .expect_err("head moved");
    assert!(
        matches!(
            &refused.error,
            CommitError::History(HistoryError::HeadMoved(_))
        ),
        "{refused:?}"
    );
    assert!(refused.locally_settled, "{refused:?}");
    assert_eq!(head(&fixture), Some(theirs), "nothing published");
    // The change is kept; over the head it now expects, it merges.
    let merged = workspace
        .commit_captured(&options(false, Some(theirs)))
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert!(merged.merge.is_some(), "merged");
    assert_eq!(
        live::committed(&fixture, "lane", "/a.txt").as_deref(),
        Some("ours!\n")
    );
    workspace.close().unwrap();
    fixture.cleanup();
}

/// The head moves after the expected head was checked and merged with:
/// the in-lane step does not merge with it and the publication refuses it.
#[test]
fn an_expected_head_that_moves_in_the_lane_is_refused() {
    tmp();
    let fixture = support::Fixture::new("expected-lane", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 63, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"ours!\n")]);
    land(&fixture, 64, b"theirs\n");
    for force in [false, true] {
        // Expected: the head as it is now, which the head read sees.
        let expected = head(&fixture);
        let (result, landed) = commit_racing_lane(
            &fixture,
            &workspace,
            &options(force, expected),
            70 + u8::from(force),
        );
        let refused = result.expect_err("head moved in the lane");
        assert!(
            matches!(
                &refused.error,
                CommitError::History(HistoryError::HeadMoved(_))
            ),
            "force {force}: {refused:?}"
        );
        assert_eq!(refused.phase, CommitPhase::Publish, "{refused:?}");
        assert!(refused.locally_settled, "{refused:?}");
        assert_eq!(head(&fixture), Some(landed), "nothing published");
    }
    assert_eq!(live::live(&workspace, "/a.txt").unwrap(), b"ours!\n");
    workspace.close().unwrap();
    fixture.cleanup();
}

/// A forced Commit over a head that was unmoved before the lane and moved
/// inside it publishes over the head the publication reads, displacing it.
#[test]
fn force_publishes_over_the_head_read_in_the_lane() {
    tmp();
    let fixture = support::Fixture::new("force-lane", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 65, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"forced\n")]);
    let (result, landed) = commit_racing_lane(&fixture, &workspace, &options(true, None), 72);
    let committed = result.unwrap_or_else(|failure| panic!("{failure:?}"));
    let Some(CommitOutcome::Committed { displaced, record }) = committed.history else {
        panic!("{:?}", committed.history);
    };
    assert_eq!(displaced, Some(landed));
    assert_eq!(head(&fixture), Some(record.id));
    assert_eq!(
        live::committed(&fixture, "lane", "/a.txt").as_deref(),
        Some("forced\n")
    );
    workspace.close().unwrap();
    fixture.cleanup();
}

/// `--force --expected` displaces exactly the expected head.
#[test]
fn force_with_expected_displaces_that_head() {
    tmp();
    let fixture = support::Fixture::new("force-expected", base);
    fixture.fork("lane");
    let workspace = live::bind(&fixture, 66, "lane");
    live::change(&workspace, &[Change::Write("/a.txt", b"forced\n")]);
    let theirs = land(&fixture, 67, b"theirs\n");
    let committed = workspace
        .commit_captured(&options(true, Some(theirs)))
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let Some(CommitOutcome::Committed { displaced, .. }) = committed.history else {
        panic!("{:?}", committed.history);
    };
    assert_eq!(displaced, Some(theirs));
    workspace.close().unwrap();
    fixture.cleanup();
}
