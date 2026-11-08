//! A Workspace Commit answers before the overlay reclamation its releases
//! ready (owner approval, 2026-11-07): the captured generation's retired
//! rows stay queued after `commit_captured`, `BoundWorkspace::reclaim` (the
//! daemon calls it after the answer) or the next job of the Workspace drains
//! them, and close deletes what is still queued with the file.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_content::filesystem::PathName;
use layerfs_history::{BranchId, CommitOutcome, HistoryName, LayerStackId, WorkspaceId};
use layerfs_store_service::store::{BindRequest, BoundWorkspace, CommitOptions};
use layerfs_workspace::{Operation, Outcome, Position, Time};

const WRITE: usize = 128 * 1024;
/// Eight writes: 32 dense rows of 32 KiB in the captured generation.
const SIZE: usize = 8 * WRITE;

/// Binds Workspace `id` on `lane`, writes a new `SIZE`-byte file `new<id>.bin` and
/// commits it. Returns the bound Workspace and the file's bytes.
fn committed(fixture: &support::Fixture, id: u8) -> (BoundWorkspace, Vec<u8>) {
    let bound = fixture
        .service
        .store
        .bind(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([id; 32]).unwrap(),
                branch: BranchId::named(
                    LayerStackId::from_authority([23; 16]),
                    &HistoryName::new("lane").unwrap(),
                ),
            },
        )
        .unwrap();
    let workspace = bound.workspace;
    // Text lines of eight bytes, so a read can check them by line number.
    let bytes = (0..SIZE / 8)
        .flat_map(|line| format!("{line:07}\n").into_bytes())
        .collect::<Vec<_>>();
    {
        let operation = workspace.operation().unwrap();
        let route = operation.workspace().route();
        let source = operation
            .overlay()
            .acquire_base_source(route, 9001)
            .unwrap();
        let view = operation.workspace().view_for_source(source).unwrap();
        let apply = |op: Operation| {
            let applied = operation
                .workspace()
                .mutate(
                    operation.overlay(),
                    operation.ports(),
                    &view,
                    op,
                    Time {
                        seconds: 123,
                        nanoseconds: 0,
                    },
                )
                .unwrap();
            let Outcome::Applied { publication, stat } = applied else {
                panic!("Workspace mutation: {applied:?}");
            };
            operation.overlay().reply_attempted(publication).unwrap();
            stat
        };
        let serial = apply(Operation::Create {
            parent: view.root_serial(),
            name: PathName::new(&format!("new{id}.bin")).unwrap(),
            mode: 0o644,
        })
        .expect("created")
        .serial;
        for (index, piece) in bytes.chunks(WRITE).enumerate() {
            apply(Operation::Write {
                serial,
                position: Position::At((index * WRITE) as u64),
                data: piece.to_vec().into(),
            });
        }
        drop(view);
        operation
            .overlay()
            .release_base_source(route, source)
            .unwrap();
    }
    let committed = workspace
        .commit_captured(&CommitOptions {
            force: false,
            message: Some("new file".into()),
        })
        .unwrap_or_else(|failure| panic!("Workspace Commit: {failure:?}"));
    assert!(matches!(
        committed.history,
        Some(CommitOutcome::Committed { .. })
    ));
    let held = committed.namespace.as_ref().expect("captured construction");
    assert!(!held.retained(), "reader and operation owner released");
    (workspace, bytes)
}

fn fixture(label: &str) -> support::Fixture {
    let fixture = support::Fixture::new(label, |source| {
        std::fs::write(source.join("keep.txt"), b"kept").unwrap();
        1
    });
    fixture.fork("lane");
    fixture
}

/// (maintenance jobs, maintenance rows) of every Workspace of the owner.
fn maintenance(fixture: &support::Fixture) -> (u64, u64) {
    let work = fixture.owner.client().diagnostics().unwrap();
    (work.maintenance_jobs, work.maintenance_rows)
}

#[test]
fn a_commit_answers_before_its_reclamation_and_reclaim_drains_it_once() {
    std::env::set_var("TMPDIR", env!("CARGO_TARGET_TMPDIR"));
    let fixture = fixture("commit-reclaim");
    let before = maintenance(&fixture);
    let (workspace, bytes) = committed(&fixture, 71);
    let answered = maintenance(&fixture);
    // The 32 retired data rows are not dropped before the answer.
    assert!(
        answered.1 - before.1 < 32,
        "rows reclaimed before the answer: {}",
        answered.1 - before.1
    );
    workspace.reclaim().unwrap();
    let reclaimed = maintenance(&fixture);
    assert!(
        reclaimed.1 - answered.1 >= 32,
        "reclaim dropped {} rows",
        reclaimed.1 - answered.1
    );
    // Nothing is left queued: a second drain runs no step.
    workspace.reclaim().unwrap();
    assert_eq!(maintenance(&fixture), reclaimed);
    assert!(fixture
        .owner
        .client()
        .maintenance_failure()
        .unwrap()
        .is_none());
    // The committed file reads back after its overlay rows are gone: its
    // size, and lines deep inside it (line n holds n - 1).
    let read = fixture.call(
        "fs.read",
        vec![
            ("branch", SdkValue::text("lane")),
            ("path", SdkValue::text("/new71.bin")),
            ("startLine", SdkValue::Int(100_000)),
            ("endLine", SdkValue::Int(100_002)),
        ],
    );
    assert_eq!(read.get_int("size"), Some(bytes.len() as i64));
    assert_eq!(read.get_text("text"), Some("0099999\n0100000\n0100001\n"));
    workspace.close().unwrap();
    fixture.cleanup();
}

#[test]
fn the_next_job_drains_what_a_commit_left_queued_and_close_deletes_the_rest() {
    std::env::set_var("TMPDIR", env!("CARGO_TARGET_TMPDIR"));
    let fixture = fixture("commit-next-job");
    let (workspace, _) = committed(&fixture, 72);
    let answered = maintenance(&fixture);
    // Any later job of the Workspace drains the queue at its end.
    let operation = workspace.operation().unwrap();
    operation
        .overlay()
        .state(operation.workspace().route())
        .unwrap();
    drop(operation);
    let drained = maintenance(&fixture);
    assert!(
        drained.1 - answered.1 >= 32,
        "the next job dropped {} rows",
        drained.1 - answered.1
    );
    workspace.close().unwrap();
    // A Workspace closed with its reclamation still queued: the file goes
    // as a whole, with no step run for it.
    let (queued, _) = committed(&fixture, 73);
    let left = maintenance(&fixture);
    queued.close().unwrap();
    assert_eq!(maintenance(&fixture), left);
    assert!(fixture
        .owner
        .client()
        .maintenance_failure()
        .unwrap()
        .is_none());
    fixture.cleanup();
}
