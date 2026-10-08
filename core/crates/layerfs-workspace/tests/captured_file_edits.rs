//! Real owner normalization with an independent final-byte/canonical oracle.
mod common;
use common::{file, fixture, Store};
use layerfs_content::{
    apply_edits, construct_runs, read_all, AuthenticatedObjects, ConstructionPolicy, ContentError,
    ContentResult, Edit, EditRequest, EditSequence, EditSource, FileRun, FileRuns,
    FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_daemon::{Command, Completion, Owner, OwnerClient, OwnerConfig, Response};
use layerfs_overlay::{
    CapturedReader, IndexedOperationRecordScope, OperationOwner, ProfileConfig, Route,
};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{
    BaseView, CanonicalClient, CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits,
    InodeSerials, Operation, Outcome, OverlayCapturedRuns, OverlayOperationRecords, Position, Time,
    Workspace, WorkspaceError, WorkspaceResult,
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const TIME: Time = Time {
    seconds: 1,
    nanoseconds: 0,
};
const CONTEXT: u32 = 0x4346_0000;
const EDITS: u32 = 0x4346_0001;
struct Objects {
    store: Store,
    calls: Mutex<BTreeMap<ObjectId, u64>>,
    refusal: Mutex<Option<(ObjectId, ContentError)>>,
}
impl AuthenticatedObjects for Objects {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        for id in ids {
            *self.calls.lock().unwrap().entry(*id).or_default() += 1;
        }
        if let Some((id, cause)) = &*self.refusal.lock().unwrap() {
            if ids.contains(id) {
                return Err(cause.clone());
            }
        }
        self.store.read_canonical_batch(ids)
    }
}
struct Serials(AtomicU64);
impl InodeSerials for Serials {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        Ok((self.0.fetch_add(count, Ordering::Relaxed), count))
    }
}
struct Case {
    owner: Option<Owner>,
    client: OwnerClient,
    workspace: Arc<Workspace>,
    objects: Arc<Objects>,
    store: Store,
    bytes: Vec<u8>,
    route: Route,
    next: Cell<u64>,
    serials: Serials,
    path: PathBuf,
}
impl Case {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let f = fixture();
        let objects = Arc::new(Objects {
            store: f.store.clone(),
            calls: Mutex::new(BTreeMap::new()),
            refusal: Mutex::new(None),
        });
        let canonical = Arc::new(CanonicalClient::with_lengths(
            objects.clone(),
            Arc::new(f.store.clone()),
            0,
        ));
        let base = BaseView::open(canonical, f.root, f.scope).unwrap();
        let path = std::env::temp_dir().join(format!(
            "layerfs-captured-edits-{}-{}.sqlite",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let owner = Owner::start(&path, ProfileConfig::default(), OwnerConfig::default()).unwrap();
        let client = owner.client();
        let opened = job(
            &client,
            None,
            Command::Open {
                incarnation: [91; 32],
                base_root: f.root.0.to_bytes(),
            },
        );
        let route = match opened.result() {
            Ok(Response::Opened(route)) => *route,
            other => panic!("{other:?}"),
        };
        drop(opened);
        Self {
            owner: Some(owner),
            client,
            workspace: Arc::new(Workspace::bind(route, base)),
            objects,
            store: f.store,
            bytes: f.bytes,
            route,
            next: Cell::new(0),
            serials: Serials(AtomicU64::new(1000)),
            path,
        }
    }
    fn request(&self) -> u64 {
        let value = self.next.get() + 1;
        self.next.set(value);
        value
    }
    fn job(&self, command: Command) -> Completion {
        job(&self.client, Some(self.route), command)
    }
    fn mutate(&self, operation: Operation) -> Option<layerfs_workspace::ViewStat> {
        let acquired = self.job(Command::AcquireBaseSource {
            owner: self.request(),
        });
        let source = match acquired.result() {
            Ok(Response::BaseSource(source)) => *source,
            other => panic!("{other:?}"),
        };
        drop(acquired);
        let view = self.workspace.view_for_source(source).unwrap();
        let outcome = self
            .workspace
            .mutate(&self.client, &self.serials, &view, operation, TIME)
            .unwrap();
        let result = match outcome {
            Outcome::Applied { publication, stat } => {
                assert!(self
                    .job(Command::ReplyAttempted(publication))
                    .result()
                    .is_ok());
                stat
            }
            Outcome::Unchanged { stat } => stat,
        };
        assert!(self
            .job(Command::ReleaseBaseSource(source))
            .result()
            .is_ok());
        result
    }
    fn write(&self, serial: u64, at: u64, bytes: &[u8]) {
        self.mutate(Operation::Write {
            serial,
            position: Position::At(at),
            data: bytes.into(),
        });
    }
    fn size(&self, serial: u64, size: u64) {
        self.mutate(Operation::SetAttributes {
            serial,
            mode: None,
            mtime: None,
            size: Some(size),
        });
    }
    fn captured(&self, file_scope: u64) -> (CapturedReader, IndexedOperationRecordScope) {
        let captured = self.job(Command::Capture);
        let capture = match captured.result() {
            Ok(Response::Captured(capture)) => *capture,
            other => panic!("{other:?}"),
        };
        drop(captured);
        let acquired = self.job(Command::AcquireCapturedReader {
            capture,
            request: self.request(),
        });
        let reader = match acquired.result() {
            Ok(Response::CapturedReader(Some(reader))) => *reader,
            other => panic!("{other:?}"),
        };
        drop(acquired);
        let acquired = self.job(Command::AcquireOperation {
            request: self.request(),
        });
        let owner = match acquired.result() {
            Ok(Response::Operation(Some(owner))) => *owner,
            other => panic!("{other:?}"),
        };
        drop(acquired);
        (reader, IndexedOperationRecordScope { owner, file_scope })
    }
    fn construct(
        &self,
        reader: CapturedReader,
        scope: IndexedOperationRecordScope,
        serial: u64,
    ) -> CapturedFileAttempt {
        let prepared =
            CapturedFileEdits::prepare(&self.workspace, &self.client, reader, serial, scope)
                .unwrap();
        let policy = ConstructionPolicy::frozen_default();
        Timing::disabled("captured-proof", |timing| {
            Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
                policy,
                &policy.capacities(),
                &mut self.store.clone(),
                timing.child("construct"),
            ))
        })
        .0
        .unwrap()
    }
    fn release(&self, custody: CapturedFileCustody, reader_released: bool) {
        let reader = custody.reader;
        let operation = custody.records.scope.owner;
        drop(custody); // First drop original credited outcomes, not their engine owners.
        if !reader_released {
            assert!(self
                .job(Command::ReleaseCapturedReader(reader))
                .result()
                .is_ok());
        }
        release_operation(self, operation);
    }
    fn oracle(&self, root: ObjectId, expected: &[u8], edits: Option<&Model>) {
        let mut bytes = Vec::new();
        Timing::disabled("oracle", |scope| {
            read_all(&self.store, root, &mut bytes, scope.child("read"))
        })
        .0
        .unwrap();
        assert_eq!(bytes, expected);
        let canonical = match edits {
            None => file(&self.store, expected),
            Some(edits) => {
                Timing::disabled("byte-edit-oracle", |scope| {
                    apply_edits(
                        ConstructionPolicy::frozen_default(),
                        &ConstructionPolicy::frozen_default().capacities(),
                        &self.store,
                        EditRequest {
                            root: edits.root,
                            edits,
                            source: edits,
                        },
                        &mut self.store.clone(),
                        scope.child("construct"),
                    )
                })
                .0
                .unwrap()
                .root
            }
        };
        assert_eq!(
            root, canonical,
            "independent final bytes and original byte-source canonical route"
        );
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            owner.stop().unwrap();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}
fn job(client: &OwnerClient, route: Option<Route>, command: Command) -> Completion {
    let pending = client
        .try_submit(route, command)
        .unwrap_or_else(|(cause, original)| panic!("original admission: {cause:?} {original:?}"));
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(original) = pending.try_complete().unwrap() {
            return original;
        }
        assert!(Instant::now() < end, "bounded original owner job");
        std::thread::yield_now();
    }
}
/// The engine thread drops its own final credit after the consumer observes a
/// completion: bound the wait for that release before an exact counter check.
fn settled(client: &OwnerClient, outstanding: usize) {
    let end = Instant::now() + Duration::from_secs(3);
    while client.diagnostics().unwrap().outstanding != outstanding {
        assert!(
            Instant::now() < end,
            "engine did not release its final credit"
        );
        std::thread::yield_now();
    }
}
fn release_operation(case: &Case, owner: OperationOwner) {
    assert!(case.job(Command::ReleaseOperation(owner)).result().is_ok());
}
struct Model {
    root: ObjectId,
    base: u64,
    bytes: Vec<u8>,
    edits: Vec<Edit>,
}
impl EditSequence for Model {
    fn base_len(&self) -> u64 {
        self.base
    }
    fn final_len(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn len(&self) -> usize {
        self.edits.len()
    }
    fn edit_at(&self, index: usize) -> ContentResult<Edit> {
        self.edits
            .get(index)
            .copied()
            .ok_or(ContentError::InvalidRecord("oracle edit index"))
    }
}
impl EditSource for Model {
    fn replacement_len(&self, index: usize) -> u64 {
        self.edits[index].replacement_len()
    }
    fn read_at(&self, index: usize, offset: u64, output: &mut [u8]) -> ContentResult<usize> {
        let edit = self.edits[index];
        let start = edit.start() + offset;
        let count = (edit.replacement_len() - offset).min(output.len() as u64) as usize;
        output[..count].copy_from_slice(&self.bytes[start as usize..start as usize + count]);
        Ok(count)
    }
}

#[test]
fn overlapping_writes_shrink_regrow_and_cross_cell_spans_match_final_bytes() {
    let case = Case::new();
    let mut expected = case.bytes.clone();
    for (at, bytes) in [
        (4087, &[0x91; 43][..]),
        (4100, &[0; 73][..]),
        (120001, &[0x19; 13][..]),
    ] {
        case.write(8, at, bytes);
        expected[at as usize..at as usize + bytes.len()].copy_from_slice(bytes);
    }
    case.size(8, 5011);
    expected.truncate(5011);
    case.size(8, 177777);
    expected.resize(177777, 0);
    case.write(8, 8190, &[0x64; 11]);
    expected[8190..8201].fill(0x64);
    let (reader, scope) = case.captured(81);
    let attempt = case.construct(reader, scope, 8);
    let result = attempt.result.as_ref().unwrap();
    let model = Model {
        root: attempt.custody.file.as_ref().unwrap().root(),
        base: case.bytes.len() as u64,
        bytes: expected.clone(),
        edits: vec![
            Edit::overwrite(4087, 4173),
            Edit::overwrite(5011, 177777),
            Edit::delete(177777, 400000),
        ],
    };
    case.oracle(result.root, &expected, Some(&model));
    assert_eq!(result.logical_len, expected.len() as u64);
    assert_eq!(attempt.custody.work.classifications, 1);
    assert!(attempt.custody.work.normalized_edits < 8);
    assert_eq!(
        case.objects
            .calls
            .lock()
            .unwrap()
            .get(&attempt.custody.file.as_ref().unwrap().root()),
        Some(&1)
    );
    case.release(attempt.custody, false);
    settled(&case.client, 0);
    assert_eq!(case.client.diagnostics().unwrap().credited_bytes, 0);
}

#[test]
fn length_growth_and_true_new_file_do_not_use_a_fabricated_base() {
    let case = Case::new();
    let original = b"original\0\xff";
    let mut expected = original.to_vec();
    case.size(2, 8201);
    expected.resize(8201, 0);
    case.write(2, 8199, b"xy");
    expected[8199..8201].copy_from_slice(b"xy");
    let serial = case
        .mutate(Operation::Create {
            parent: 1,
            name: common::name("fresh"),
            mode: 0o644,
        })
        .unwrap()
        .serial;
    case.size(serial, 10011);
    case.write(serial, 4095, b"new");
    let mut fresh = vec![0; 10011];
    fresh[4095..4098].copy_from_slice(b"new");
    let (reader, scope) = case.captured(82);
    let attempt = case.construct(reader, scope, 2);
    case.oracle(attempt.result.as_ref().unwrap().root, &expected, None);
    assert_eq!(
        attempt.custody.work.normalized_edits, 1,
        "one trailing extension"
    );
    // Both final files already belong to this one captured frontier. Distinct
    // file scopes share the actual operation/reader until both constructions
    // finish; releasing a reader does not install or resolve its Capture.
    let fresh_scope = IndexedOperationRecordScope {
        file_scope: 83,
        ..scope
    };
    let fresh_attempt = case.construct(reader, fresh_scope, serial);
    case.oracle(fresh_attempt.result.as_ref().unwrap().root, &fresh, None);
    assert!(fresh_attempt.custody.file.is_none());
    assert_eq!(fresh_attempt.custody.work.classifications, 0);
    drop(attempt.custody);
    drop(fresh_attempt.custody);
    assert!(case
        .job(Command::ReleaseCapturedReader(reader))
        .result()
        .is_ok());
    release_operation(&case, scope.owner);
}

#[test]
fn same_byte_overwrite_retains_original_root_with_one_classification() {
    let case = Case::new();
    case.write(8, 4101, &case.bytes[4101..4173]);
    let (reader, scope) = case.captured(84);
    let attempt = case.construct(reader, scope, 8);
    let original = attempt.custody.file.as_ref().unwrap().root();
    assert_eq!(attempt.result.as_ref().unwrap().root, original);
    assert_eq!(attempt.custody.work.normalized_edits, 1);
    assert_eq!(
        attempt.custody.work.source_resets, 0,
        "no construction after exact no-op"
    );
    assert_eq!(case.objects.calls.lock().unwrap().get(&original), Some(&1));
    case.release(attempt.custody, false);
}

#[test]
fn many_edits_use_numbered_records_after_install_and_reads_survive_close() {
    let case = Case::new();
    let mut expected = case.bytes.clone();
    for index in 0..129 {
        let at = 5 + index * 2000;
        case.write(8, at, &[0x77]);
        expected[at as usize] = 0x77;
    }
    let (reader, scope) = case.captured(85);
    let prepared =
        CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope).unwrap();
    assert!(case
        .job(Command::Install {
            capture: reader.capture(),
            root: reader.root()
        })
        .result()
        .is_ok());
    let policy = ConstructionPolicy::frozen_default();
    let attempt = Timing::disabled("retained", |timing| {
        Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
            policy,
            &policy.capacities(),
            &mut case.store.clone(),
            timing.child("construct"),
        ))
    })
    .0
    .unwrap();
    let model = Model {
        root: attempt.custody.file.as_ref().unwrap().root(),
        base: case.bytes.len() as u64,
        bytes: expected.clone(),
        edits: (0..129)
            .map(|index| Edit::overwrite(5 + index * 2000, 6 + index * 2000))
            .collect(),
    };
    case.oracle(
        attempt.result.as_ref().unwrap().root,
        &expected,
        Some(&model),
    );
    assert_eq!(attempt.custody.work.normalized_edits, 129);
    let mut count = 0;
    let mut after = None;
    loop {
        let keys = case
            .client
            .operation_record_keys_after(scope, EDITS, after)
            .unwrap()
            .value;
        if keys.is_empty() {
            break;
        }
        for key in &keys {
            let row = case
                .client
                .operation_record_get(
                    scope,
                    layerfs_overlay::IndexedOperationRecordKey {
                        kind: EDITS,
                        key: *key,
                    },
                )
                .unwrap()
                .value
                .unwrap();
            assert_eq!(row.len(), 25);
            count += 1;
        }
        after = keys.last().copied();
    }
    assert_eq!(count, 129);
    assert!(case.job(Command::Close).result().is_ok());
    let reply = case
        .client
        .captured_run_step(layerfs_overlay::CapturedRunCursor::new(reader, 8, 0, 400000).unwrap())
        .unwrap();
    assert!(
        matches!(reply.step,layerfs_overlay::CapturedRunStep::Window {read,next} if read.base_root==Some(reader.root()) && next.reader()==reader)
    );
    case.release(attempt.custody, false);
}

struct Zeros(u64);
impl FileRuns for Zeros {
    fn read_run(&mut self, _: &mut [u8]) -> ContentResult<FileRun> {
        if self.0 == 0 {
            Ok(FileRun::End)
        } else {
            let length = self.0;
            self.0 = 0;
            Ok(FileRun::Zero(length))
        }
    }
}
#[test]
fn new_hole_above_four_gib_uses_bounded_original_run_work() {
    let case = Case::new();
    let serial = case
        .mutate(Operation::Create {
            parent: 1,
            name: common::name("huge"),
            mode: 0o644,
        })
        .unwrap()
        .serial;
    let length = (1_u64 << 32) + 17;
    case.size(serial, length);
    let (reader, scope) = case.captured(86);
    let attempt = case.construct(reader, scope, serial);
    let result = attempt.result.as_ref().unwrap();
    let policy = ConstructionPolicy::frozen_default();
    let oracle = Timing::disabled("zero-oracle", |timing| {
        construct_runs(
            policy,
            &policy.capacities(),
            &mut Zeros(length),
            &mut case.store.clone(),
            timing.child("construct"),
        )
    })
    .0
    .unwrap();
    assert_eq!(result.root, oracle.file.root);
    assert_eq!(result.logical_len, length);
    assert_eq!(attempt.custody.work.normalized_edits, 1);
    assert_eq!(attempt.custody.work.normalization_steps, 1);
    assert_eq!(attempt.custody.work.source_steps, 1);
    case.release(attempt.custody, false);
}

#[test]
fn first_provider_absence_denial_identity_and_length_mismatch_never_prove_newness() {
    for cause in [
        ContentError::MissingObject,
        ContentError::IdentityMismatch,
        ContentError::ProviderFailure {
            what: "original authenticated denial",
        },
    ] {
        let case = Case::new();
        let root = case
            .workspace
            .base()
            .unwrap()
            .inode(8)
            .unwrap()
            .value
            .content_root;
        *case.objects.refusal.lock().unwrap() = Some((root, cause.clone()));
        let (reader, scope) = case.captured(87);
        let custody =
            match CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope) {
                Ok(_) => panic!("no alternate new-file source"),
                Err(original) => original,
            };
        assert!(
            matches!(&custody.failure,Some(WorkspaceError::Content(original)) if *original==cause)
        );
        assert_eq!(custody.records.work.calls, 0);
        assert_eq!(custody.work.normalization_steps, 0);
        case.release(custody, false);
    }
    let case = Case::new();
    let root = case
        .workspace
        .base()
        .unwrap()
        .inode(8)
        .unwrap()
        .value
        .content_root;
    case.store.lengths.lock().unwrap().insert(root, 3);
    let (reader, scope) = case.captured(88);
    let custody = match CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope)
    {
        Ok(_) => panic!("cheap length cannot replace classification"),
        Err(original) => original,
    };
    assert!(matches!(
        &custody.failure,
        Some(WorkspaceError::Content(ContentError::LengthMismatch {
            expected: 3,
            actual: 400000
        }))
    ));
    assert_eq!(custody.records.work.calls, 0);
    case.release(custody, false);
}

#[test]
fn occupied_context_and_retired_reader_keep_original_completions_without_replay() {
    let case = Case::new();
    case.write(8, 17, b"different");
    let (reader, scope) = case.captured(89);
    let prepared =
        CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope).unwrap();
    let custody = match CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope)
    {
        Ok(_) => panic!("Missing context guard"),
        Err(original) => original,
    };
    let Some(WorkspaceError::Service(original)) = &custody.records.failure else {
        panic!("original deciding completion")
    };
    assert!(original.downcast_ref::<Completion>().is_some());
    assert_eq!(custody.work.normalization_steps, 0);
    assert!(case.client.diagnostics().unwrap().credited_bytes > 0);
    drop(custody);
    assert!(case
        .job(Command::ReleaseCapturedReader(reader))
        .result()
        .is_ok());
    let policy = ConstructionPolicy::frozen_default();
    let attempt = Timing::disabled("retired", |timing| {
        Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
            policy,
            &policy.capacities(),
            &mut case.store.clone(),
            timing.child("construct"),
        ))
    })
    .0
    .unwrap();
    assert!(attempt.result.is_err());
    let Some(WorkspaceError::Service(original)) = &attempt.custody.failure else {
        panic!("original captured failure")
    };
    let done = original.downcast_ref::<Completion>().unwrap();
    assert!(matches!(
        done.result(),
        Err(layerfs_daemon::OwnerError::Overlay(
            layerfs_overlay::OverlayError::Stale
        ))
    ));
    assert_eq!(attempt.custody.work.source_steps, 1);
    case.release(attempt.custody, true);
}

struct RejectFinal {
    store: Store,
    children: usize,
}
impl FinalizedConsumer for RejectFinal {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if object.role() == ObjectRole::FileState {
            return Err(ContentError::OutputRejected);
        }
        self.children += 1;
        self.store.accept(object)
    }
}
#[test]
fn accepted_children_remain_with_consumer_when_final_file_root_is_refused() {
    let case = Case::new();
    case.write(8, 101, &[0xee; 8193]);
    let (reader, scope) = case.captured(90);
    let prepared =
        CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope).unwrap();
    let policy = ConstructionPolicy::frozen_default();
    let mut consumer = RejectFinal {
        store: case.store.clone(),
        children: 0,
    };
    let attempt = Timing::disabled("refused-root", |timing| {
        Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
            policy,
            &policy.capacities(),
            &mut consumer,
            timing.child("construct"),
        ))
    })
    .0
    .unwrap();
    assert_eq!(attempt.result, Err(ContentError::OutputRejected));
    assert!(consumer.children > 0);
    assert!(matches!(
        &attempt.custody.failure,
        Some(WorkspaceError::Content(ContentError::OutputRejected))
    ));
    let context = case
        .client
        .operation_record_get(
            scope,
            layerfs_overlay::IndexedOperationRecordKey {
                kind: CONTEXT,
                key: [0; 32],
            },
        )
        .unwrap()
        .value
        .unwrap();
    assert_eq!(
        &context[..2],
        &[1, 1],
        "original sealed normalization remains"
    );
    case.release(attempt.custody, false);
}

struct InheritedPort<'a> {
    client: &'a OwnerClient,
    window: bool,
    oversized: Option<bool>,
    record_oversized: bool,
    continuation: Option<bool>,
}
impl OverlayCapturedRuns for InheritedPort<'_> {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<layerfs_overlay::Inode>> {
        self.client.captured_inode(reader, serial)
    }
    fn captured_run_step(
        &self,
        cursor: layerfs_overlay::CapturedRunCursor,
    ) -> WorkspaceResult<layerfs_overlay::CapturedRunReply> {
        let mut original = self.client.captured_run_step(cursor)?;
        if let Some(restart) = self.continuation {
            assert_eq!(
                original.work.metadata_rows, 1,
                "actual Owner metadata row, not a claimed counter"
            );
            let supplied = if restart {
                let next = match &original.step {
                    layerfs_overlay::CapturedRunStep::Window { next, .. }
                    | layerfs_overlay::CapturedRunStep::Gap { next, .. }
                    | layerfs_overlay::CapturedRunStep::Continue(next) => *next,
                    layerfs_overlay::CapturedRunStep::End => panic!("nonempty original input"),
                };
                layerfs_overlay::CapturedRunCursor::new(
                    next.reader(),
                    next.serial(),
                    cursor.offset(),
                    next.logical_size(),
                )?
            } else {
                cursor
            };
            original.step = layerfs_overlay::CapturedRunStep::Continue(supplied);
            return Ok(original);
        }
        if let layerfs_overlay::CapturedRunStep::Gap { offset, next, .. } = original.step {
            let end = cursor.logical_size();
            let next = next.seek_forward(end)?;
            original.step = if self.window {
                let length = (end - offset) as usize;
                let mut data = Vec::with_capacity(if self.oversized == Some(false) {
                    layerfs_overlay::CELL_BYTES + 1
                } else {
                    length
                });
                data.resize(length, 0xa5);
                let mut inherited = Vec::with_capacity(if self.oversized == Some(true) {
                    layerfs_overlay::MASK_BYTES + 1
                } else {
                    length.div_ceil(8)
                });
                inherited.resize(length.div_ceil(8), 0xff);
                layerfs_overlay::CapturedRunStep::Window {
                    read: Box::new(layerfs_overlay::LocalRead {
                        kind: layerfs_overlay::InodeKind::File,
                        size: end,
                        offset,
                        data,
                        inherited,
                        span: Some((offset, end)),
                        base_root: Some(cursor.reader().root()),
                    }),
                    next,
                }
            } else {
                layerfs_overlay::CapturedRunStep::Gap {
                    kind: layerfs_overlay::CapturedGap::Inherited,
                    offset,
                    length: end - offset,
                    next,
                }
            };
        }
        Ok(original)
    }
}
impl OverlayOperationRecords for InheritedPort<'_> {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: layerfs_overlay::IndexedOperationRecordKey,
    ) -> WorkspaceResult<layerfs_workspace::OperationRecordReply<bool>> {
        self.client.operation_record_contains(scope, key)
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: layerfs_overlay::IndexedOperationRecordKey,
    ) -> WorkspaceResult<layerfs_workspace::OperationRecordReply<Option<Vec<u8>>>> {
        let mut original = self.client.operation_record_get(scope, key)?;
        if self.record_oversized {
            if let Some(value) = original.value.take() {
                let mut oversized = Vec::with_capacity(layerfs_overlay::OPERATION_RECORD_BYTES + 1);
                oversized.extend_from_slice(&value);
                original.copies.bytes += value.len() as u64;
                original.copies.allocations += 1;
                original.copies.retained_bytes = oversized.capacity();
                original.value = Some(oversized);
            }
        }
        Ok(original)
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<layerfs_overlay::IndexedOperationRecordChange>,
    ) -> WorkspaceResult<
        layerfs_workspace::OperationRecordReply<layerfs_workspace::OperationRecordApply>,
    > {
        self.client.operation_record_apply(scope, changes)
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<layerfs_workspace::OperationRecordReply<Vec<[u8; 32]>>> {
        self.client.operation_record_keys(scope, kind, excluded)
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<layerfs_workspace::OperationRecordReply<Vec<[u8; 32]>>> {
        self.client.operation_record_keys_after(scope, kind, after)
    }
}
#[test]
fn inherited_gap_and_window_crossing_authenticated_eof_are_zero_without_base_fallback() {
    for window in [false, true] {
        let case = Case::new();
        case.size(2, 13);
        let (reader, scope) = case.captured(91);
        // The external public adapter preserves the actual Owner's binding and
        // metadata unit, while presenting inherited bytes across immutable EOF.
        // Nonzero window bytes behind inherited mask bits must never be consumed.
        let provider = InheritedPort {
            client: &case.client,
            window,
            oversized: None,
            record_oversized: false,
            continuation: None,
        };
        let prepared =
            CapturedFileEdits::prepare(&case.workspace, &provider, reader, 2, scope).unwrap();
        let policy = ConstructionPolicy::frozen_default();
        let attempt = Timing::disabled("inherited-eof", |timing| {
            Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
                policy,
                &policy.capacities(),
                &mut case.store.clone(),
                timing.child("construct"),
            ))
        })
        .0
        .unwrap();
        let mut expected = b"original\0\xff".to_vec();
        expected.resize(13, 0);
        case.oracle(attempt.result.as_ref().unwrap().root, &expected, None);
        assert_eq!(attempt.custody.work.normalized_edits, 1);
        case.release(attempt.custody, false);
    }
}

#[test]
fn logical_close_allows_retained_reads_but_refuses_normalization_record_mutation() {
    let case = Case::new();
    case.write(8, 17, b"changed");
    let (reader, scope) = case.captured(92);
    assert!(case.job(Command::Close).result().is_ok());
    let read = case
        .client
        .captured_run_step(layerfs_overlay::CapturedRunCursor::new(reader, 8, 0, 400000).unwrap())
        .unwrap();
    assert!(
        matches!(read.step,layerfs_overlay::CapturedRunStep::Window {read,..} if read.base_root==Some(reader.root()))
    );
    let custody = match CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope)
    {
        Ok(_) => panic!("closed Workspace cannot acquire mutable normalization records"),
        Err(original) => original,
    };
    let Some(WorkspaceError::Service(original)) = &custody.records.failure else {
        panic!("original closed Completion")
    };
    let done = original.downcast_ref::<Completion>().unwrap();
    assert!(matches!(
        done.result(),
        Err(layerfs_daemon::OwnerError::Overlay(
            layerfs_overlay::OverlayError::Closed
        ))
    ));
    assert_eq!(custody.work.normalization_steps, 0);
    case.release(custody, false);
}

#[test]
fn changed_context_or_missing_edit_is_terminal_before_consumer_acceptance() {
    for context_changed in [true, false] {
        let case = Case::new();
        case.write(8, 17, b"changed");
        let (reader, scope) = case.captured(93);
        let prepared =
            CapturedFileEdits::prepare(&case.workspace, &case.client, reader, 8, scope).unwrap();
        let key = layerfs_overlay::IndexedOperationRecordKey {
            kind: if context_changed { CONTEXT } else { EDITS },
            key: [0; 32],
        };
        let expected = case
            .client
            .operation_record_get(scope, key)
            .unwrap()
            .value
            .unwrap();
        let mut changed = expected.clone();
        changed[2] ^= 1;
        assert!(matches!(
            case.client
                .operation_record_apply(
                    scope,
                    vec![layerfs_overlay::IndexedOperationRecordChange {
                        key,
                        expected: layerfs_overlay::OperationRecordExpectedValue::ExactBytes(
                            expected
                        ),
                        value: context_changed.then_some(changed),
                    }]
                )
                .unwrap()
                .value,
            layerfs_workspace::OperationRecordApply::Applied
        ));
        let policy = ConstructionPolicy::frozen_default();
        let mut consumer = RejectFinal {
            store: case.store.clone(),
            children: 0,
        };
        let attempt = Timing::disabled("changed-context", |timing| {
            Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
                policy,
                &policy.capacities(),
                &mut consumer,
                timing.child("construct"),
            ))
        })
        .0
        .unwrap();
        assert!(matches!(
            attempt.result,
            Err(ContentError::InvalidRecord(
                "captured file context" | "missing captured edit"
            ))
        ));
        assert_eq!(consumer.children, 0);
        assert_eq!(attempt.custody.work.source_steps, 0);
        case.release(attempt.custody, false);
    }
}

#[test]
fn prepare_after_actual_binding_install_rebinds_only_the_retained_original_root() {
    use layerfs_content::filesystem::{
        update_filesystem, FilesystemInput, FilesystemObjects, FilesystemResources, InodeUpdate,
    };
    let case = Case::new();
    case.write(8, 17, b"original capture");
    let mut expected = case.bytes.clone();
    expected[17..33].copy_from_slice(b"original capture");
    let (reader, scope) = case.captured(94);
    let original = case.workspace.base().unwrap();
    let mut value = original.inode(8).unwrap().value;
    value.content_root = file(&case.store, b"later installed content");
    let updates = [InodeUpdate { serial: 8, value }];
    let mut sink = case.store.clone();
    let mut objects = FilesystemObjects::new(&case.store, &mut sink);
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(original.identity()),
            scope: original.root().scope(),
            root_serial: original.root().root_inode().serial(),
            directories: &[],
            inodes: &updates,
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let input = case
        .workspace
        .prepare_base_install(reader.capture(), next)
        .unwrap();
    assert!(case
        .job(Command::InstallPrepared {
            workspace: case.workspace.clone(),
            input
        })
        .result()
        .is_ok());
    assert_eq!(case.workspace.base().unwrap().identity(), next);
    let attempt = case.construct(reader, scope, 8);
    assert_eq!(
        attempt.custody.base.as_ref().unwrap().identity(),
        original.identity()
    );
    assert_eq!(attempt.custody.file.as_ref().unwrap().logical_len(), 400000);
    let model = Model {
        root: attempt.custody.file.as_ref().unwrap().root(),
        base: 400000,
        bytes: expected.clone(),
        edits: vec![Edit::overwrite(17, 33)],
    };
    case.oracle(
        attempt.result.as_ref().unwrap().root,
        &expected,
        Some(&model),
    );
    case.release(attempt.custody, false);
}

#[test]
fn small_window_with_oversized_retained_capacity_refuses_before_pending_or_output() {
    for mask in [false, true] {
        let case = Case::new();
        case.size(2, 13);
        let (reader, scope) = case.captured(95);
        let provider = InheritedPort {
            client: &case.client,
            window: true,
            oversized: Some(mask),
            record_oversized: false,
            continuation: None,
        };
        let admitted = case.client.diagnostics().unwrap().admitted;
        let objects = case.store.objects.lock().unwrap().len();
        let custody = match CapturedFileEdits::prepare(&case.workspace, &provider, reader, 2, scope)
        {
            Ok(_) => panic!("oversized retained window cannot become a plan"),
            Err(original) => original,
        };
        assert!(matches!(
            &custody.failure,
            Some(WorkspaceError::Content(ContentError::InvalidRecord(
                "captured file run shape"
            )))
        ));
        assert_eq!(custody.work.normalization_steps, 1);
        assert_eq!(custody.work.source_steps, 0);
        assert_eq!(custody.work.normalized_edits, 0);
        assert_eq!(
            custody.records.work.calls, 1,
            "only the initial Missing context; no edit or sealing jobs"
        );
        assert_eq!(
            case.client.diagnostics().unwrap().admitted,
            admitted + 3,
            "one exact inode point, initial context and original window; no later job"
        );
        assert_eq!(
            case.store.objects.lock().unwrap().len(),
            objects,
            "no consumer output"
        );
        case.release(custody, false);
    }
    let case = Case::new();
    case.size(2, 13);
    let (reader, scope) = case.captured(96);
    let provider = InheritedPort {
        client: &case.client,
        window: false,
        oversized: None,
        record_oversized: true,
        continuation: None,
    };
    let prepared =
        CapturedFileEdits::prepare(&case.workspace, &provider, reader, 2, scope).unwrap();
    let admitted = case.client.diagnostics().unwrap().admitted;
    let mut consumer = RejectFinal {
        store: case.store.clone(),
        children: 0,
    };
    let policy = ConstructionPolicy::frozen_default();
    let attempt = Timing::disabled("oversized-record", |timing| {
        Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
            policy,
            &policy.capacities(),
            &mut consumer,
            timing.child("construct"),
        ))
    })
    .0
    .unwrap();
    assert!(
        matches!(&attempt.result, Err(ContentError::BoundedCapacityExceeded {
        what: "captured record returned capacity", limit: 65536, actual,
    }) if *actual > 65536)
    );
    assert_eq!(consumer.children, 0);
    assert_eq!(attempt.custody.work.source_steps, 0);
    assert_eq!(
        case.client.diagnostics().unwrap().admitted,
        admitted + 1,
        "original context point only; no later source/backing job"
    );
    case.release(attempt.custody, false);
}

#[test]
fn unchanged_or_restarted_continuation_refuses_without_spinning_or_output() {
    for restart in [false, true] {
        let case = Case::new();
        case.write(8, 17, b"local");
        let (reader, scope) = case.captured(97);
        let provider = InheritedPort {
            client: &case.client,
            window: false,
            oversized: None,
            record_oversized: false,
            continuation: Some(restart),
        };
        let admitted = case.client.diagnostics().unwrap().admitted;
        let objects = case.store.objects.lock().unwrap().len();
        let custody = match CapturedFileEdits::prepare(&case.workspace, &provider, reader, 8, scope)
        {
            Ok(_) => panic!("metadata row count cannot prove unchanged/restarted progress"),
            Err(original) => original,
        };
        assert!(matches!(
            &custody.failure,
            Some(WorkspaceError::Content(ContentError::InvalidRecord(
                "captured file run shape"
            )))
        ));
        assert_eq!(custody.work.normalization_steps, 1);
        assert_eq!(custody.work.normalized_edits, 0);
        assert_eq!(custody.records.work.calls, 1);
        assert_eq!(
            case.client.diagnostics().unwrap().admitted,
            admitted + 3,
            "one original metadata unit and no later call"
        );
        assert_eq!(case.store.objects.lock().unwrap().len(), objects);
        case.release(custody, false);
    }
}
