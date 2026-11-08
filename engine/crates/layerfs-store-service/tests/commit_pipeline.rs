//! The product Commit builds on one thread and saves on another
//! (`store/pipeline.rs`). It must give the roots and stored bytes the same
//! constructor gives on one thread, and the first failure must end both
//! sides: a refused object stops the producer, a failed construction
//! abandons the Save, and nothing is published either way.
//!
//! The failures are real Store refusals: from the start of the `construct`
//! phase a second connection holds the Store's write lock, so the next
//! write fails `Busy` with no busy handler.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_content::{filesystem::PathName, ContentError, FinalizedConsumer};
use layerfs_history::{BranchId, CommitOutcome, HistoryName, LayerStackId, WorkspaceId};
use layerfs_store_service::store::{
    digests::StoreDigests, BindRequest, BoundWorkspace, CommitError, CommitFailure, CommitOptions,
    CommitPhase, CommitSuccess,
};
use layerfs_store_service::telemetry::{mark_phases, Recording};
use layerfs_workspace::{CapturedNamespace, Operation, Outcome, Position, Time, WorkspaceError};
use std::{cell::Cell, rc::Rc, sync::Once};

const MIB: usize = 1024 * 1024;

/// Stores live below Cargo's target tmp: a Linux build container's `/tmp`
/// is overlayfs, which a SQLite Store refuses.
fn tmp() {
    static SET: Once = Once::new();
    SET.call_once(|| std::env::set_var("TMPDIR", env!("CARGO_TARGET_TMPDIR")));
}

fn random(len: usize, mut seed: u64) -> Vec<u8> {
    let mut out = vec![0u8; len];
    for word in out.chunks_mut(8) {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        word.copy_from_slice(&seed.to_le_bytes()[..word.len()]);
    }
    out
}

fn base(source: &std::path::Path) -> usize {
    std::fs::write(source.join("keep.bin"), random(MIB, 7)).unwrap();
    std::fs::write(source.join("small.txt"), b"small base file\n").unwrap();
    2
}

fn bind(fixture: &support::Fixture, id: u8) -> BoundWorkspace {
    fixture
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
        .unwrap()
        .workspace
}

/// Creates each named file at the root, or overwrites it from `at`, in
/// 128 KiB writes as the kernel sends them.
fn write(workspace: &BoundWorkspace, files: &[(&str, u64, &[u8])]) {
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
    for (name, at, bytes) in files {
        let name = PathName::new(name).unwrap();
        let serial = match view.lookup(operation.overlay(), view.root_serial(), &name) {
            Ok(found) => found.serial,
            Err(_) => {
                apply(Operation::Create {
                    parent: view.root_serial(),
                    name,
                    mode: 0o644,
                })
                .expect("created")
                .serial
            }
        };
        for (index, piece) in bytes.chunks(128 * 1024).enumerate() {
            apply(Operation::Write {
                serial,
                position: Position::At(at + (index * 128 * 1024) as u64),
                data: piece.to_vec().into(),
            });
        }
    }
    drop(view);
    operation
        .overlay()
        .release_base_source(route, source)
        .unwrap();
}

fn options() -> CommitOptions {
    CommitOptions {
        force: false,
        message: Some("pipeline".into()),
    }
}

/// The same captured constructor on the calling thread, writing straight
/// into the Save, through the caller-constructor driver.
fn single_thread(fixture: &support::Fixture, workspace: &BoundWorkspace) -> CommitSuccess {
    let store = &fixture.service.store;
    let operation = workspace.operation().unwrap();
    let route = operation.workspace().route();
    let owners = Cell::new(None);
    let committed = workspace
        .commit(|save, capture, _| {
            let request = capture.generation.number() as u64;
            let reader = operation
                .overlay()
                .acquire_captured_reader(route, capture, request)
                .map_err(CommitError::Owner)?;
            let owner = operation
                .overlay()
                .acquire_operation(route, request)
                .map_err(CommitError::Owner)?;
            owners.set(Some((reader, owner)));
            let policy = store.policy().construction();
            let digests = StoreDigests(store.history());
            let mut sink = save.sink();
            let attempt =
                CapturedNamespace::new(operation.workspace(), operation.overlay(), reader, owner)
                    .with_digests(&digests)
                    .construct_untimed(
                        policy,
                        &policy.capacities(),
                        save,
                        &mut sink as &mut dyn FinalizedConsumer,
                    );
            Ok(attempt.result.expect("single-thread construction").root)
        })
        .unwrap_or_else(|failure| panic!("single-thread Commit: {failure:?}"));
    let (reader, owner) = owners.take().expect("owners acquired");
    operation
        .overlay()
        .release_captured_reader(route, reader)
        .unwrap();
    operation.overlay().release_operation(route, owner).unwrap();
    committed
}

fn root(committed: &CommitSuccess) -> [u8; 32] {
    match &committed.history {
        Some(CommitOutcome::Committed { record, .. }) => record.root.to_bytes(),
        other => panic!("not committed: {other:?}"),
    }
}

#[test]
fn pipelined_commit_matches_single_thread() {
    tmp();
    let host = installed::Fixture::built("pipe-base", base);
    let path = host.config.path.clone();
    let ours = support::Fixture::open(installed::Fixture::cloned("pipe-ours", &path, 2));
    let theirs = support::Fixture::open(installed::Fixture::cloned("pipe-theirs", &path, 2));
    for fixture in [&ours, &theirs] {
        fixture.fork("lane");
    }
    let (piped, single) = (bind(&ours, 41), bind(&theirs, 41));
    // Large: a new 12 MiB file (many Save waves, a full queue) and an edit
    // of the base file; then small: a few small files.
    let big = random(12 * MIB, 11);
    let edit = random(64 * 1024, 13);
    let large: &[(&str, u64, &[u8])] = &[("big.bin", 0, &big), ("keep.bin", 4096, &edit)];
    let small: &[(&str, u64, &[u8])] = &[
        ("a.txt", 0, b"alpha\n"),
        ("b.txt", 0, b"beta\n"),
        ("small.txt", 0, b"changed\n"),
    ];
    for changes in [large, small] {
        write(&piped, changes);
        write(&single, changes);
        let ours = piped.commit_captured(&options()).unwrap();
        let theirs = single_thread(&theirs, &single);
        assert_eq!(root(&ours), root(&theirs));
        assert_eq!(ours.saved.inserted, theirs.saved.inserted);
        assert_eq!(ours.saved.canonical_bytes, theirs.saved.canonical_bytes);
        assert_eq!(ours.saved.stored_bytes, theirs.saved.stored_bytes);
        assert_eq!(ours.saved.packs, theirs.saved.packs);
    }
    for (workspace, fixture) in [(piped, ours), (single, theirs)] {
        workspace.close().unwrap();
        fixture.cleanup();
    }
    host.cleanup();
}

/// From the start of `construct` a second connection holds the Store's
/// write lock; the guard ends it.
struct Locked {
    connection: Rc<rusqlite::Connection>,
    _recording: Recording,
}
impl Locked {
    fn at_construct(path: &std::path::Path) -> Self {
        let connection = Rc::new(rusqlite::Connection::open(path).unwrap());
        let recording = Recording::start();
        let held = connection.clone();
        mark_phases(Some(Box::new(move |name, end| {
            if name == "construct" && !end {
                held.execute_batch("BEGIN EXCLUSIVE").unwrap();
            }
        })));
        Self {
            connection,
            _recording: recording,
        }
    }
}
impl Drop for Locked {
    fn drop(&mut self) {
        mark_phases(None);
        let _ = self.connection.execute_batch("ROLLBACK");
    }
}

fn head(fixture: &support::Fixture) -> Option<layerfs_history::CommitId> {
    fixture
        .service
        .store
        .history()
        .branch_snapshot(BranchId::named(
            LayerStackId::from_authority([23; 16]),
            &HistoryName::new("lane").unwrap(),
        ))
        .unwrap()
        .unwrap()
        .branch
        .head_commit
}

fn failed(fixture: &support::Fixture, workspace: &BoundWorkspace) -> Box<CommitFailure> {
    let before = head(fixture);
    let failure = {
        let _locked = Locked::at_construct(&fixture.host.config.path);
        workspace
            .commit_captured(&options())
            .expect_err("Commit under a held write lock")
    };
    assert_eq!(failure.phase, CommitPhase::Construct, "{failure:?}");
    assert!(failure.published.is_none(), "{failure:?}");
    assert!(failure.saved.is_none(), "the Save never finished");
    assert_eq!(head(fixture), before, "nothing published");
    failure
}

#[test]
fn save_failure_stops_the_producer() {
    tmp();
    let fixture = support::Fixture::new("pipe-save-fails", base);
    fixture.fork("lane");
    let workspace = bind(&fixture, 42);
    // Far more than one Save wave: the first wave's write is refused while
    // the producer is still inside the file.
    let big = random(32 * MIB, 17);
    write(&workspace, &[("big.bin", 0, &big)]);
    let failure = failed(&fixture, &workspace);
    // The Save's refusal is the first cause; the producer only saw its
    // hand-over refused and stopped inside the file.
    let CommitError::Construction { original, storage } = &failure.error else {
        panic!("not a Save failure: {failure:?}");
    };
    assert!(!storage.is_unknown_outcome(), "{storage:?}");
    assert!(
        matches!(
            original.as_ref(),
            CommitError::Content {
                error: ContentError::OutputRejected,
                ..
            }
        ),
        "{original:?}"
    );
    let namespace = failure.namespace.as_ref().expect("constructor ran");
    let work = namespace.work.expect("producer work");
    assert_eq!(work.files_constructed, 0, "stopped inside the file");
    fixture.cleanup();
}

#[test]
fn construction_failure_abandons_the_save() {
    tmp();
    let fixture = support::Fixture::new("pipe-construct-fails", base);
    fixture.fork("lane");
    let workspace = bind(&fixture, 43);
    // Less than one Save wave stays in the Save's pending batch; the
    // producer's own write, the new chunked file's digest, is the first one
    // refused.
    let chunked = random(MIB, 19);
    write(
        &workspace,
        &[("a.txt", 0, b"alpha\n"), ("c.bin", 0, &chunked)],
    );
    let failure = failed(&fixture, &workspace);
    let CommitError::Workspace(WorkspaceError::Service(cause)) = &failure.error else {
        panic!("not the producer's own failure: {failure:?}");
    };
    assert!(
        cause
            .downcast_ref::<layerfs_history::HistoryError>()
            .is_some(),
        "{cause:?}"
    );
    fixture.cleanup();
}
