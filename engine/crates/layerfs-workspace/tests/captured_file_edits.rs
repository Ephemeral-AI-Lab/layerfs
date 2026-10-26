//! Real owner normalization with an independent final-byte/canonical oracle.
mod captured_support;
mod common;
use captured_support::*;
use common::{file, Store};
use layerfs_content::{
    construct_runs, ConstructionPolicy, ContentError, ContentResult, Edit, FileRun, FileRuns,
    FinalizedConsumer, FinalizedObject, ObjectRole,
};
use layerfs_daemon::OwnerClient;
use layerfs_overlay::{CapturedReader, IndexedOperationRecordScope};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{
    CapturedFileAttempt, CapturedFileEdits, Operation, OverlayCapturedRuns,
    OverlayOperationRecords, WorkspaceError, WorkspaceResult,
};

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
    case.job(|job_client: &layerfs_daemon::OwnerClient, job_route| {
        job_client.release_captured_reader(job_route, reader)
    });
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
    case.job(|job_client: &layerfs_daemon::OwnerClient, job_route| {
        job_client.install_prepared(
            job_route,
            case.workspace.clone(),
            case.workspace
                .prepare_base_install(reader.capture(), case.workspace.base().unwrap().identity())
                .unwrap(),
        )
    });
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
    case.job(|job_client: &layerfs_daemon::OwnerClient, job_route| job_client.close(job_route));
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
    // A new file has no surviving base bytes, so normalization visits nothing.
    assert_eq!(attempt.custody.work.normalization_steps, 0);
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
        &[2, 1],
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
                    | layerfs_overlay::CapturedRunStep::Local { next, .. }
                    | layerfs_overlay::CapturedRunStep::Reference { next, .. }
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
                    layerfs_overlay::RUN_BYTES + 1
                } else {
                    length
                });
                data.resize(length, 0xa5);
                let mut inherited = Vec::with_capacity(if self.oversized == Some(true) {
                    layerfs_overlay::RUN_BYTES / 8 + 1
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
                        references: Vec::new(),
                        referenced: Vec::new(),
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
    case.job(|job_client: &layerfs_daemon::OwnerClient, job_route| {
        job_client.install_prepared(job_route, case.workspace.clone(), input)
    });
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
        let admitted = case.client.diagnostics().unwrap().jobs.iter().sum::<u64>();
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
            case.client.diagnostics().unwrap().jobs.iter().sum::<u64>(),
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
    let admitted = case.client.diagnostics().unwrap().jobs.iter().sum::<u64>();
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
        case.client.diagnostics().unwrap().jobs.iter().sum::<u64>(),
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
        let admitted = case.client.diagnostics().unwrap().jobs.iter().sum::<u64>();
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
            case.client.diagnostics().unwrap().jobs.iter().sum::<u64>(),
            admitted + 3,
            "one original metadata unit and no later call"
        );
        assert_eq!(case.store.objects.lock().unwrap().len(), objects);
        case.release(custody, false);
    }
}
