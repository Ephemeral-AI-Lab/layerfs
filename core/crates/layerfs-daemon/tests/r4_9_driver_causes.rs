//! Row R4-9 at driver scope: a failure of each kind of provider call the
//! captured namespace producer makes, driven through the unchanged public
//! Store Commit driver `BoundWorkspace::commit`, over a real Disposable Store
//! and the real local owner, without a mount.
//!
//! The producer is given the real owner client behind a test adapter, in the
//! test's own Commit closure. The adapter forwards every call unchanged
//! except the one selected, which fails in one of two ways:
//!
//! - `Opaque`: the adapter does not forward the call and answers with a
//!   failure of its own type. The driver cannot know what an unknown service
//!   failure did, so it must classify the attempt as uncertain: no local
//!   resolution, the capture kept, nothing published, and the Workspace's
//!   Commit ownership kept, so a later Commit is refused before any effect.
//! - `Engine`: the adapter releases, through the same public owner client,
//!   the owner the call depends on (the captured reader for a reader call,
//!   the operation owner for a record call) and then forwards the call. The
//!   failure is the real engine's own refusal in its original completion,
//!   which the driver must classify as definite and resolve locally once.
//!   The foreign release is a job no product path issues.
//!
//! In both, the closure hands the driver the first original cause exactly as
//! the product constructor's classification does, and leaves the reader and
//! the operation owner unreleased: releasing them is the caller's step after
//! the driver has classified the failure, as in the product route. Every
//! definite case ends with a later Commit through the product constructor
//! that is read back completely from a fresh bind; an uncertain case cannot,
//! and ends with the refusal of that later Commit and exact custody.
//!
//! This is driver scope. The product constructor `commit_captured` accepts
//! no provider, so these causes cannot be injected into it without a hook.
#[allow(dead_code)]
#[path = "support/captured_drive.rs"]
mod drive;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use drive::{evidence, job, live, settled, Do, Rig, Session};
use layerfs_content::ObjectId;
use layerfs_daemon::{
    store::{BoundWorkspace, CommitError, CommitFailure, CommitPhase, CommitSuccess},
    Command, Completion, OwnerClient, OwnerError, Response,
};
use layerfs_history::{CommitHistoryRequest, CommitRecord, CommitStagedOutcome};
use layerfs_overlay::{
    Capture, CapturedReader, CapturedRunCursor, CapturedRunReply, DirectoryEntry,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope, Inode,
    OperationOwner, Route,
};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{
    CapturedNamespace, CapturedNamespaceAttempt, OperationRecordApply, OperationRecordReply,
    OverlayCapturedNamespace, OverlayCapturedRuns, OverlayOperationRecords, WorkspaceError,
    WorkspaceResult,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt, fs,
    os::unix::fs::symlink,
    path::Path,
};

/// The provider calls of one attempt, by kind. Record calls are split by
/// scope: the namespace's own (file scope 0) and a changed file's.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Call {
    InodePage,
    InodePoint,
    NamePage,
    ParentPage,
    NamePoint,
    Symlink,
    RunStep,
    NamespaceApply,
    NamespaceGet,
    NamespaceContains,
    NamespaceKeys,
    FileApply,
    FileRead,
}
const CALLS: [Call; 13] = [
    Call::InodePage,
    Call::InodePoint,
    Call::NamePage,
    Call::ParentPage,
    Call::NamePoint,
    Call::Symlink,
    Call::RunStep,
    Call::NamespaceApply,
    Call::NamespaceGet,
    Call::NamespaceContains,
    Call::NamespaceKeys,
    Call::FileApply,
    Call::FileRead,
];
/// How the selected call fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum How {
    Opaque,
    Engine,
}
/// The adapter's own failure, recognizable by downcast.
#[derive(Debug)]
struct Injected(Call);
impl fmt::Display for Injected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Injected {}
/// Which owner the adapter released before forwarding the selected call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Released {
    Reader,
    Operation,
}

/// The real owner client behind a recorder that fails one selected call.
struct Failing<'a> {
    client: &'a OwnerClient,
    route: Route,
    reader: CapturedReader,
    fault: Cell<Option<(Call, u64, How)>>,
    counts: RefCell<BTreeMap<Call, u64>>,
    order: RefCell<Vec<Call>>,
    failed: Cell<Option<Call>>,
    after_failure: Cell<u64>,
    released: Cell<Option<Released>>,
}
impl<'a> Failing<'a> {
    fn new(
        client: &'a OwnerClient,
        route: Route,
        reader: CapturedReader,
        fault: Option<(Call, u64, How)>,
    ) -> Self {
        Self {
            client,
            route,
            reader,
            fault: Cell::new(fault),
            counts: RefCell::new(BTreeMap::new()),
            order: RefCell::new(Vec::new()),
            failed: Cell::new(None),
            after_failure: Cell::new(0),
            released: Cell::new(None),
        }
    }
    /// Counts one call. `Err` is the adapter's own answer; `Ok` forwards.
    fn enter(&self, call: Call, owner: Option<OperationOwner>) -> WorkspaceResult<()> {
        if self.failed.get().is_some() {
            self.after_failure.set(self.after_failure.get() + 1);
        }
        let index = {
            let mut counts = self.counts.borrow_mut();
            let count = counts.entry(call).or_default();
            *count += 1;
            *count - 1
        };
        self.order.borrow_mut().push(call);
        match self.fault.get() {
            Some((wanted, nth, how)) if wanted == call && nth == index => {
                self.fault.set(None);
                self.failed.set(Some(call));
                match how {
                    How::Opaque => Err(WorkspaceError::Service(Box::new(Injected(call)))),
                    How::Engine => {
                        let (command, released) = match owner {
                            Some(owner) => (Command::ReleaseOperation(owner), Released::Operation),
                            None => (
                                Command::ReleaseCapturedReader(self.reader),
                                Released::Reader,
                            ),
                        };
                        let done = job(self.client, self.route, command);
                        assert!(
                            matches!(done.result(), Ok(Response::Done)),
                            "{call:?}: the foreign release: {done:?}"
                        );
                        drop(done);
                        self.released.set(Some(released));
                        Ok(())
                    }
                }
            }
            _ => Ok(()),
        }
    }
    fn record(scope: IndexedOperationRecordScope, own: Call, file: Call) -> Call {
        if scope.file_scope == 0 {
            own
        } else {
            file
        }
    }
}
impl OverlayCapturedRuns for Failing<'_> {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        self.enter(Call::InodePoint, None)?;
        OverlayCapturedRuns::captured_inode(self.client, reader, serial)
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        self.enter(Call::RunStep, None)?;
        OverlayCapturedRuns::captured_run_step(self.client, cursor)
    }
}
impl OverlayCapturedNamespace for Failing<'_> {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>> {
        self.enter(Call::InodePage, None)?;
        OverlayCapturedNamespace::captured_inode_page(self.client, reader, after)
    }
    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.enter(Call::NamePage, None)?;
        OverlayCapturedNamespace::captured_directory_entry_page(self.client, reader, after)
    }
    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.enter(Call::ParentPage, None)?;
        OverlayCapturedNamespace::captured_directory_entries(self.client, reader, parent, after)
    }
    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        self.enter(Call::NamePoint, None)?;
        OverlayCapturedNamespace::captured_directory_entry(self.client, reader, parent, name)
    }
    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>> {
        self.enter(Call::Symlink, None)?;
        OverlayCapturedNamespace::captured_symlink(self.client, reader, serial)
    }
}
impl OverlayOperationRecords for Failing<'_> {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        self.enter(
            Self::record(scope, Call::NamespaceContains, Call::FileRead),
            Some(scope.owner),
        )?;
        OverlayOperationRecords::operation_record_contains(self.client, scope, key)
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        self.enter(
            Self::record(scope, Call::NamespaceGet, Call::FileRead),
            Some(scope.owner),
        )?;
        OverlayOperationRecords::operation_record_get(self.client, scope, key)
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        self.enter(
            Self::record(scope, Call::NamespaceApply, Call::FileApply),
            Some(scope.owner),
        )?;
        OverlayOperationRecords::operation_record_apply(self.client, scope, changes)
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.enter(
            Self::record(scope, Call::NamespaceKeys, Call::FileRead),
            Some(scope.owner),
        )?;
        OverlayOperationRecords::operation_record_keys(self.client, scope, kind, excluded)
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.enter(
            Self::record(scope, Call::NamespaceKeys, Call::FileRead),
            Some(scope.owner),
        )?;
        OverlayOperationRecords::operation_record_keys_after(self.client, scope, kind, after)
    }
}

/// The custody slot that held the one original failure of a failed attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kept {
    /// `custody.failure`: a reader call or a point of the namespace itself.
    Namespace,
    /// `custody.records.failure`: a record job of the namespace's own scope.
    NamespaceRecords,
    /// `custody.file`, in the file custody's own `failure`.
    File,
    /// `custody.file`, in the file custody's `records.failure`.
    FileRecords,
}
/// What one producer attempt left, read before anything is released.
#[derive(Debug)]
struct Produced {
    reader: CapturedReader,
    owner: OperationOwner,
    counts: BTreeMap<Call, u64>,
    order: Vec<Call>,
    failed: Option<Call>,
    after_failure: u64,
    released: Option<Released>,
    /// Every custody slot holding a failure; exactly one after a failure.
    kept: Vec<Kept>,
    label: Option<String>,
}
type Committed = Result<CommitSuccess, Box<CommitFailure>>;

/// The test's Commit closure: acquire the exact reader and one operation
/// owner, run the producer once into the driver's Save over the adapter, and
/// hand the driver the first original cause as what it was. On success the
/// reader and then the owner are released; on failure neither is.
fn commit(
    rig: &Rig,
    workspace: &BoundWorkspace,
    request: u64,
    fault: Option<(Call, u64, How)>,
) -> (Committed, Option<Produced>) {
    let operation = workspace.operation().unwrap();
    let route = workspace.route();
    let policy = rig.store.policy().construction();
    let mut produced = None;
    let result = workspace.commit(|save, capture, _| {
        let overlay = operation.overlay();
        let reader = match job(
            overlay,
            route,
            Command::AcquireCapturedReader { capture, request },
        )
        .result()
        {
            Ok(Response::CapturedReader(Some(reader))) => *reader,
            other => panic!("{other:?}"),
        };
        let owner = match job(overlay, route, Command::AcquireOperation { request }).result() {
            Ok(Response::Operation(Some(owner))) => *owner,
            other => panic!("{other:?}"),
        };
        let adapter = Failing::new(overlay, route, reader, fault);
        let mut sink = save.sink();
        let CapturedNamespaceAttempt {
            result,
            mut custody,
        } = Timing::disabled("commit.namespace", |scope| {
            Ok::<_, CommitError>(
                CapturedNamespace::new(operation.workspace(), &adapter, reader, owner).construct(
                    policy,
                    &policy.capacities(),
                    save,
                    &mut sink,
                    scope.child("construct"),
                ),
            )
        })
        .0
        .unwrap();
        assert_eq!((custody.reader, custody.operation), (reader, owner));
        let mut kept = Vec::new();
        if custody.failure.is_some() {
            kept.push(Kept::Namespace);
        }
        if custody.records.failure.is_some() {
            kept.push(Kept::NamespaceRecords);
        }
        if let Some(file) = custody.file.as_ref() {
            assert_eq!(file.records.scope.owner, owner);
            assert_eq!(file.records.scope.file_scope, file.serial);
            match (file.failure.is_some(), file.records.failure.is_some()) {
                (true, false) => kept.push(Kept::File),
                (false, true) => kept.push(Kept::FileRecords),
                other => panic!("{other:?} inside the file custody: {file:?}"),
            }
        }
        produced = Some(Produced {
            reader,
            owner,
            counts: adapter.counts.borrow().clone(),
            order: adapter.order.borrow().clone(),
            failed: adapter.failed.get(),
            after_failure: adapter.after_failure.get(),
            released: adapter.released.get(),
            kept,
            label: result.as_ref().err().map(|label| format!("{label:?}")),
        });
        // The first original cause: namespace or reader first, else the
        // failing file's own or its record scope's, else the namespace
        // record scope's.
        let original = custody
            .failure
            .take()
            .or_else(|| {
                custody
                    .file
                    .as_mut()
                    .and_then(|file| file.failure.take().or_else(|| file.records.failure.take()))
            })
            .or_else(|| custody.records.failure.take());
        drop(custody);
        match (result, original) {
            (Ok(built), original) => {
                assert!(original.is_none(), "a result with a retained cause");
                assert!(job(overlay, route, Command::ReleaseCapturedReader(reader))
                    .result()
                    .is_ok());
                assert!(job(overlay, route, Command::ReleaseOperation(owner))
                    .result()
                    .is_ok());
                Ok(built.root)
            }
            // An owner job goes back as what it was: its completion, or the
            // refusal that never admitted it.
            (Err(_), Some(WorkspaceError::Service(service))) => {
                Err(match service.downcast::<Completion>() {
                    Ok(completion) => CommitError::Completion(completion),
                    Err(other) => match other.downcast::<OwnerError>() {
                        Ok(owner) => CommitError::Owner(*owner),
                        Err(other) => CommitError::Workspace(WorkspaceError::Service(other)),
                    },
                })
            }
            (Err(_), Some(WorkspaceError::Content(error))) => Err(CommitError::Content {
                error,
                provider: None,
            }),
            (Err(_), Some(original)) => Err(CommitError::Workspace(original)),
            (Err(error), None) => Err(CommitError::Content {
                error,
                provider: None,
            }),
        }
    });
    (result, produced)
}
fn record(success: &CommitSuccess) -> CommitRecord {
    match &success.history {
        CommitStagedOutcome::Committed(record) => record.clone(),
        other => panic!("{other:?}"),
    }
}
fn history(rig: &Rig) -> Vec<CommitRecord> {
    let page = rig
        .store
        .history()
        .commit_history(&CommitHistoryRequest {
            branch: rig.f.branch,
            start: None,
            cursor: None,
            limit: 32,
        })
        .unwrap();
    assert!(page.continuation.is_none());
    page.records
}
/// The published root as a fresh bind sees it, walked completely.
fn rebound(rig: &Rig, tag: u8, root: ObjectId) -> model::Flat {
    let fresh = rig.bind(tag);
    assert_eq!(fresh.snapshot().unwrap().effective_root, root);
    let operation = fresh.operation().unwrap();
    let flat = model::canonical(operation.client(), root);
    drop(operation);
    rig.close(&fresh);
    flat
}
/// What the engine itself holds for one request and for the capture.
fn held(
    rig: &Rig,
    workspace: &BoundWorkspace,
    request: u64,
) -> (
    Option<CapturedReader>,
    Option<OperationOwner>,
    Option<Capture>,
) {
    let client = rig.owner.client();
    let route = workspace.route();
    let reader = match job(&client, route, Command::RetainedCapturedReader { request }).result() {
        Ok(Response::CapturedReader(kept)) => *kept,
        other => panic!("{other:?}"),
    };
    let owner = match job(&client, route, Command::RetainedOperation { request }).result() {
        Ok(Response::Operation(kept)) => *kept,
        other => panic!("{other:?}"),
    };
    let capture = match job(&client, route, Command::RetainedCapture).result() {
        Ok(Response::RetainedCapture(kept)) => *kept,
        other => panic!("{other:?}"),
    };
    (reader, owner, capture)
}
fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|n| (n as u32).wrapping_mul(31).wrapping_add(u32::from(seed)) as u8)
        .collect()
}
fn tree(source: &Path) {
    for directory in [".git", "cache", "cache/sub", "output", "keep"] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    for (path, bytes) in [
        (".git/index", pattern(3, 300_000)),
        ("top.txt", b"top level\n".to_vec()),
        ("cache/c1", b"cached one\n".to_vec()),
        ("cache/sub/c2", pattern(9, 5_000)),
        ("output/o1", b"built\n".to_vec()),
        ("keep/k", b"kept\n".to_vec()),
    ] {
        fs::write(source.join(path), bytes).unwrap();
    }
    fs::hard_link(source.join("top.txt"), source.join("alias")).unwrap();
    symlink("top.txt", source.join("old-link")).unwrap();
}
/// One capture that needs every kind of provider call: a changed base file,
/// a chunked new file, a symlink, new and removed names and a moved
/// directory.
fn scenario(session: &mut Session<'_>) {
    let (changed, first, second) = ([0x42; 30_000], [0x17; 120_000], [0x71; 100_000]);
    session.all(&[
        Do::Write(".git/index", 150_000, &changed),
        Do::Create("new", 0o644),
        Do::Write("new", 0, &first),
        Do::Write("new", 120_000, &second),
        Do::Symlink("link", b"new"),
        Do::Mkdir("dir", 0o755),
        Do::Create("dir/a", 0o600),
        Do::Mkdir("dir/empty", 0o700),
        Do::Remove("alias"),
        Do::Rename("cache", "output/c"),
        Do::Chmod("output", 0o750),
    ]);
}
/// The slot each call kind's failure must be kept in. Inode points come from
/// two callers: the name pass asks one per finished parent before the first
/// inode page, and each file's own preparation asks its inode afterwards.
fn slot_of(call: Call, nth: u64, parent_points: u64) -> Kept {
    match call {
        Call::InodePage | Call::NamePage | Call::ParentPage | Call::NamePoint | Call::Symlink => {
            Kept::Namespace
        }
        Call::InodePoint if nth < parent_points => Kept::Namespace,
        Call::InodePoint | Call::RunStep => Kept::File,
        Call::NamespaceApply
        | Call::NamespaceGet
        | Call::NamespaceContains
        | Call::NamespaceKeys => Kept::NamespaceRecords,
        Call::FileApply | Call::FileRead => Kept::FileRecords,
    }
}

/// The unfailed attempt of the scenario through the same closure: its call
/// counts, and how many inode points belong to the name pass.
fn counted() -> (BTreeMap<Call, u64>, u64) {
    let rig = Rig::new("r4-9-count", tree);
    let first = rig.bind(1);
    let mut session = Session::new(&first, rig.base.clone());
    scenario(&mut session);
    let expected = session.model.flat();
    let (result, produced) = commit(&rig, &first, 1, None);
    let produced = produced.unwrap();
    let root = match &result {
        Ok(success) => record(success).root,
        Err(failure) => panic!("the unfailed attempt: {failure:?}"),
    };
    drop(result);
    assert_eq!((produced.failed, produced.after_failure), (None, 0));
    assert!(produced.kept.is_empty() && produced.label.is_none());
    for call in CALLS {
        assert!(
            produced.counts.get(&call).copied().unwrap_or(0) > 0,
            "{call:?} was never needed: {:?}",
            produced.counts
        );
    }
    let first_page = produced
        .order
        .iter()
        .position(|call| *call == Call::InodePage)
        .unwrap();
    let parent_points = produced.order[..first_page]
        .iter()
        .filter(|call| **call == Call::InodePoint)
        .count() as u64;
    assert!(parent_points > 0 && parent_points < produced.counts[&Call::InodePoint]);
    model::assert_same(&expected, &rebound(&rig, 2, root), "the unfailed attempt");
    evidence(format_args!(
        "R4-9 driver COUNT calls={:?} parent_points={parent_points} total={}",
        produced.counts,
        produced.order.len()
    ));
    settled(&rig.owner.client(), 0);
    rig.close(&first);
    drop(first);
    rig.finish();
    (produced.counts, parent_points)
}

/// One failed call of the scenario through the driver and what follows it.
fn failed(call: Call, nth: u64, how: How, parent_points: u64) {
    let what = format!("{how:?} {call:?} #{nth}");
    let rig = Rig::new(&format!("r4-9-{how:?}-{call:?}-{nth}"), tree);
    let first = rig.bind(1);
    let original = first.snapshot().unwrap();
    let mut session = Session::new(&first, rig.base.clone());
    scenario(&mut session);
    let expected = session.model.flat();
    let client = rig.owner.client();
    let route = first.route();

    let (result, produced) = commit(&rig, &first, 1, Some((call, nth, how)));
    let failure = match result {
        Err(failure) => failure,
        Ok(success) => panic!("{what}: the Commit succeeded: {:?}", success.history),
    };
    let produced = produced.expect("the closure ran");
    // The producer: the selected call was made, nothing followed it, and
    // its one original cause is in the slot of its kind.
    assert_eq!(produced.failed, Some(call), "{what}: never made");
    assert_eq!(
        produced.after_failure, 0,
        "{what}: a request followed the failure"
    );
    let slot = slot_of(call, nth, parent_points);
    assert_eq!(produced.kept, [slot], "{what}: {produced:?}");
    // The driver: the failure is at construction, with nothing published.
    assert_eq!(failure.phase, CommitPhase::Construct, "{what}");
    assert!(
        failure.published.is_none() && failure.intent.is_none() && failure.saved.is_none(),
        "{what}: {failure:?}"
    );
    assert!(failure.namespace.is_none(), "a caller's constructor");
    let capture = failure.capture.expect("the capture");
    let class = match how {
        How::Opaque => {
            // An unknown service failure: the driver cannot know what it did.
            match &failure.error {
                CommitError::Workspace(WorkspaceError::Service(service)) => {
                    let injected = service.downcast_ref::<Injected>().expect("the adapter's");
                    assert_eq!(injected.0, call, "{what}");
                }
                other => panic!("{what}: {other:?}"),
            }
            assert_eq!(produced.released, None);
            // Uncertain: no resolution was attempted and the capture stays.
            assert!(
                !failure.locally_settled
                    && failure.local.is_none()
                    && failure.local_error.is_none(),
                "{what}: {failure:?}"
            );
            "uncertain"
        }
        How::Engine => {
            // The engine's own refusal, in its original completion.
            let refusal = match &failure.error {
                CommitError::Completion(done) => match done.result() {
                    Err(error) => format!("{error:?}"),
                    Ok(other) => panic!("{what}: the call was answered: {other:?}"),
                },
                other => panic!("{what}: {other:?}"),
            };
            assert_eq!(
                produced.released,
                Some(
                    if slot == Kept::NamespaceRecords || slot == Kept::FileRecords {
                        Released::Operation
                    } else {
                        Released::Reader
                    }
                ),
                "{what}"
            );
            assert_eq!(refusal, "Overlay(Stale)", "{what}");
            // Definite: resolved locally, once.
            assert!(
                failure.locally_settled && failure.local_error.is_none(),
                "{what}: {failure:?}"
            );
            assert!(matches!(
                failure.local.as_ref().map(Completion::result),
                Some(Ok(Response::Done))
            ));
            "definite"
        }
    };
    let error = format!("{:?}", failure.error);
    drop(failure);
    // An uncertain attempt keeps the Workspace's Commit ownership: the driver
    // clears it only once the failure is settled (`store/settle.rs`).
    assert_eq!(first.commit_in_flight(), how == How::Opaque, "{what}");
    assert!(rig
        .store
        .history()
        .stage(first.identity())
        .unwrap()
        .is_none());
    assert_eq!(first.snapshot().unwrap(), original);
    assert!(history(&rig).is_empty());
    // Exact custody in the engine, before the caller's release step.
    let (reader, owner, kept_capture) = held(&rig, &first, 1);
    let wanted = match (how, produced.released) {
        (How::Opaque, _) => (Some(produced.reader), Some(produced.owner), Some(capture)),
        (How::Engine, Some(Released::Reader)) => (None, Some(produced.owner), None),
        (How::Engine, Some(Released::Operation)) => (Some(produced.reader), None, None),
        (How::Engine, None) => unreachable!(),
    };
    assert_eq!((reader, owner, kept_capture), wanted, "{what}: custody");
    evidence(format_args!(
        "R4-9 driver {what}: class={class} error={} slot={slot:?} label={:?} calls={} after_failure=0 published=None stage=None engine(reader={} owner={} capture={})",
        &error[..error.len().min(160)],
        produced.label,
        produced.order.len(),
        reader.is_some(),
        owner.is_some(),
        kept_capture.is_some()
    ));
    let done = |command| {
        let done = job(&client, route, command);
        assert!(
            matches!(done.result(), Ok(Response::Done)),
            "{what}: {done:?}"
        );
    };
    if how == How::Opaque {
        // Nothing is replayed: a later Commit of the Workspace is refused
        // before any effect, and the retained custody is untouched.
        let again = first
            .commit_captured()
            .expect_err("a Commit over retained uncertain custody");
        assert!(
            matches!(again.error, CommitError::InFlight),
            "{what}: {again:?}"
        );
        assert!(
            again.capture.is_none()
                && again.published.is_none()
                && again.namespace.is_none()
                && again.local.is_none()
                && !again.locally_settled,
            "{what}: {again:?}"
        );
        drop(again);
        assert_eq!(held(&rig, &first, 1), wanted, "{what}: custody after");
        assert!(history(&rig).is_empty());
        model::assert_same(&expected, &live(&first), "live view under custody");
        // Teardown by the test, outside the claim: History itself says that
        // nothing was published, so the capture is resolved and the reader
        // and the owner released through the owner client. The Workspace
        // stays fenced; the driver is not told.
        done(Command::ResolveFailed(capture));
        done(Command::ReleaseCapturedReader(produced.reader));
        done(Command::ReleaseOperation(produced.owner));
        assert_eq!(held(&rig, &first, 1), (None, None, None), "{what}");
        model::assert_same(&expected, &live(&first), "live view after teardown");
        assert!(first.commit_in_flight());
        settled(&client, 0);
        rig.close(&first);
        drop(first);
        rig.finish();
        return;
    }
    // The caller's step after a settled failure, as in the product route:
    // the reader and then the operation owner, whichever are still held.
    if reader.is_some() {
        done(Command::ReleaseCapturedReader(produced.reader));
    }
    if owner.is_some() {
        done(Command::ReleaseOperation(produced.owner));
    }
    assert_eq!(held(&rig, &first, 1), (None, None, None), "{what}");
    model::assert_same(&expected, &live(&first), "live view after the failure");
    // A later Commit through the product constructor is exact.
    let later = first
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{what}: the later Commit: {failure:?}"));
    let published = record(&later);
    assert_eq!(published.parent, original.branch.head_commit);
    drop(later);
    model::assert_same(
        &expected,
        &rebound(&rig, 2, published.root),
        "fresh bind after the failure",
    );
    assert_eq!(history(&rig), vec![published]);
    settled(&client, 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}
/// The first call of every kind, and the inode point at the boundary of its
/// two callers.
fn sweep(how: How) {
    let (counts, parent_points) = counted();
    let mut cases = 0;
    for call in CALLS {
        let mut selected = vec![0];
        if call == Call::InodePoint {
            selected.push(parent_points);
        }
        for nth in selected {
            assert!(nth < counts[&call]);
            failed(call, nth, how, parent_points);
            cases += 1;
        }
    }
    assert_eq!(cases, 14);
    evidence(format_args!(
        "R4-9 driver SWEEP how={how:?} cases={cases} call_kinds={} after={}",
        CALLS.len(),
        match how {
            How::Opaque => "workspace_fenced,later_commit=InFlight,custody_untouched,live=exact",
            How::Engine => "owners_released_by_caller,later_commit=Committed,fresh_bind=exact",
        }
    ));
}

/// The adapter answers the selected call itself with an unknown failure: the
/// driver classifies every kind as uncertain, keeps the capture and stays
/// fenced.
#[test]
fn r4_9_an_unknown_failure_of_each_provider_call_is_uncertain_at_the_driver() {
    sweep(How::Opaque);
}

/// The engine itself refuses the selected call after a foreign release: the
/// driver classifies every kind as definite and resolves the capture once.
#[test]
fn r4_9_an_engine_refusal_of_each_provider_call_is_definite_at_the_driver() {
    sweep(How::Engine);
}
