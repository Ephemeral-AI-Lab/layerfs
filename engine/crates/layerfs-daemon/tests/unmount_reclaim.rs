//! A terminal unmount does not run the reclamation a Commit left queued
//! (#679): those rows live in the Workspace's own overlay file, which close
//! deletes as a whole. Before the fix the unmount's dirty check was the first
//! job after the Commit and drained the whole queue before answering. A
//! maintenance hold that ends gives the queue back to the next job.
//! Counted from the owner's maintenance totals; nothing is timed.
#[path = "../../layerfs-store-service/tests/support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../../layerfs-store-service/tests/support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{CommitControl, Reply, Request, UnmountControl, WorkspaceToken};
use layerfs_content::filesystem::PathName;
use layerfs_daemon::control::Service;
use layerfs_history::{BranchId, HistoryName, LayerStackId, WorkspaceId};
use layerfs_workspace::{Operation, Outcome, Position, Time};

const LANE: &str = "lane";
const WRITE: usize = 128 * 1024;
/// Eight writes: 32 dense rows of 32 KiB that the Commit retires.
const SIZE: usize = 8 * WRITE;

struct Fixture {
    sdk: support::Fixture,
    service: Service,
}
impl Fixture {
    fn cleanup(self) {
        drop(self.service);
        self.sdk.cleanup();
    }
    /// (maintenance jobs, maintenance rows) of every Workspace of the owner.
    fn maintenance(&self) -> (u64, u64) {
        let work = self.sdk.owner.client().diagnostics().unwrap();
        (work.maintenance_jobs, work.maintenance_rows)
    }
}

fn fixture(label: &str) -> Fixture {
    std::env::set_var("TMPDIR", env!("CARGO_TARGET_TMPDIR"));
    let sdk = support::Fixture::new(label, |source| {
        std::fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    sdk.fork(LANE);
    let service = Service::new(sdk.service.store.clone(), &sdk.owner);
    Fixture { sdk, service }
}

/// Mounts Workspace `id` on the lane and writes a new `SIZE`-byte file
/// `/new<id>.bin` through the daemon's operation, without committing.
fn written(fixture: &Fixture, id: u8) -> WorkspaceToken {
    let bound = fixture
        .service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([id; 32]).unwrap(),
            branch: BranchId::named(
                LayerStackId::from_authority([23; 16]),
                &HistoryName::new(LANE).unwrap(),
            ),
        })
        .unwrap();
    let Reply::Bound { token, .. } = bound.reply else {
        panic!("binding: {:?}", bound.reply);
    };
    {
        let operation = fixture.service.operation(token).unwrap();
        let route = operation.workspace().route();
        let source = operation
            .overlay()
            .acquire_base_source(route, 9000 + u64::from(id))
            .unwrap();
        let view = operation.workspace().view_for_source(source).unwrap();
        let now = Time {
            seconds: 123,
            nanoseconds: 0,
        };
        let apply = |op| {
            let outcome = operation
                .workspace()
                .mutate(operation.overlay(), operation.ports(), &view, op, now)
                .unwrap();
            let Outcome::Applied { publication, stat } = outcome else {
                panic!("mutation: {outcome:?}");
            };
            operation.overlay().reply_attempted(publication).unwrap();
            stat
        };
        let serial = apply(Operation::Create {
            parent: view.root_serial(),
            name: PathName::new(&format!("new{id}.bin")).unwrap(),
            mode: 0o644,
        })
        .expect("created inode")
        .serial;
        for index in 0..SIZE / WRITE {
            apply(Operation::Write {
                serial,
                position: Position::At((index * WRITE) as u64),
                data: vec![id; WRITE].into(),
            });
        }
        drop(view);
        operation
            .overlay()
            .release_base_source(route, source)
            .unwrap();
    }
    token
}

/// `written`, then committed through control.
fn committed(fixture: &Fixture, id: u8) -> WorkspaceToken {
    let token = written(fixture, id);
    let done = fixture
        .service
        .execute_control(&Request::Commit(CommitControl {
            token,
            force: false,
            message: None,
        }))
        .unwrap_or_else(|failure| panic!("Commit: {failure:?}"));
    assert!(
        matches!(done.reply, Reply::Committed(_)),
        "{:?}",
        done.reply
    );
    token
}

#[test]
fn an_unmount_after_a_commit_runs_none_of_its_reclamation() {
    let fixture = fixture("unmount-reclaim");
    let token = committed(&fixture, 81);
    let answered = fixture.maintenance();
    // The checked unmount: its dirty check is the first job after the Commit.
    let closed = fixture
        .service
        .execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: false,
        }))
        .unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    drop(closed);
    assert_eq!(
        fixture.maintenance(),
        answered,
        "the unmount ran queued reclamation (jobs, rows)"
    );
    assert!(fixture
        .sdk
        .owner
        .client()
        .maintenance_failure()
        .unwrap()
        .is_none());
    fixture.cleanup();
}

#[test]
fn a_released_hold_leaves_the_queue_to_the_next_job() {
    let fixture = fixture("unmount-hold");
    let token = committed(&fixture, 82);
    let answered = fixture.maintenance();
    let operation = fixture.service.operation(token).unwrap();
    let route = operation.workspace().route();
    let roster = fixture.sdk.owner.client();
    // Holds nest: while either lives, jobs and `reclaim` run no step.
    let first = roster.hold_maintenance(route).unwrap();
    let second = roster.hold_maintenance(route).unwrap();
    operation.overlay().state(route).unwrap();
    drop(first);
    operation.overlay().reclaim(route).unwrap();
    assert_eq!(fixture.maintenance(), answered, "held (jobs, rows)");
    drop(second);
    // Released: the next job drains the queue at its end, as before.
    operation.overlay().state(route).unwrap();
    let drained = fixture.maintenance();
    assert!(
        drained.1 - answered.1 >= 32,
        "the next job dropped {} rows",
        drained.1 - answered.1
    );
    drop(operation);
    let closed = fixture
        .service
        .execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: false,
        }))
        .unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    drop(closed);
    fixture.cleanup();
}

fn unmount(fixture: &Fixture, token: WorkspaceToken, discard: bool) -> Reply {
    fixture
        .service
        .execute_control(&Request::Unmount(UnmountControl { token, discard }))
        .unwrap()
        .reply
}

/// A Commit that failed while an unmount held the Workspace leaves its capture
/// folding back (`consolidating`) with the fold queued: the active counts are
/// 0, yet the changes are there. The checked unmount refuses it, and the
/// refusal drains the queue, so the fold does not wait for a later job.
#[test]
fn a_failed_commits_folding_capture_refuses_and_the_refusal_folds_it() {
    let fixture = fixture("unmount-folding");
    let token = written(&fixture, 83);
    let operation = fixture.service.operation(token).unwrap();
    let route = operation.workspace().route();
    let roster = fixture.sdk.owner.client();
    // The interleaving behind the review finding: another unmount's hold is
    // live while the failed Commit resolves its capture.
    let other = roster.hold_maintenance(route).unwrap();
    let capture = operation.overlay().capture(route).unwrap();
    operation.overlay().resolve_failed(route, capture).unwrap();
    let state = operation.overlay().state(route).unwrap();
    assert!(state.consolidating.is_some(), "{state:?}");
    assert_eq!((state.dirty_inodes, state.dirty_directory_entries), (0, 0));
    let Reply::Dirty(report) = unmount(&fixture, token, false) else {
        panic!("a folding capture was closed as clean");
    };
    assert!(report.truncated, "{report:?}");
    drop(other);
    // Still folding: the refused unmount held it. Its refusal drains the
    // queue once its own hold is gone.
    let before = fixture.maintenance();
    assert!(matches!(unmount(&fixture, token, false), Reply::Dirty(_)));
    assert!(
        fixture.maintenance().0 > before.0,
        "the refusal ran no step"
    );
    let state = operation.overlay().state(route).unwrap();
    assert!(state.consolidating.is_none(), "{state:?}");
    assert!(state.dirty_inodes > 0, "{state:?}");
    // Folded: the report names the path again.
    let Reply::Dirty(report) = unmount(&fixture, token, false) else {
        panic!("the folded change was closed as clean");
    };
    assert_eq!(report.paths, vec!["/new83.bin".to_owned()]);
    assert!(!report.truncated, "{report:?}");
    drop(operation);
    assert_eq!(unmount(&fixture, token, true), Reply::Unmounted(token));
    fixture.cleanup();
}
