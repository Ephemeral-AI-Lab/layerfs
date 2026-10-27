//! A file the base does not hold, built on the direct route (record 033 F2)
//! and on the normalized-edit record route, from the same captured input:
//! the same root, length, final bytes and emitted object identities.
mod captured_support;
mod common;
use captured_support::*;
use common::Store;
use layerfs_content::{
    ConstructionPolicy, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_overlay::{ClaimedRange, IndexedOperationRecordScope, WriteClaims};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{
    CapturedFileAttempt, CapturedFileEdits, Operation, OverlayOperationRecords, Position, WriteData,
};

/// The base file the fixture commits, and its bytes.
const BASE_FILE: u64 = 8;

/// Every object a construction emitted, in order, kept in the shared store.
struct Emitted {
    ids: Vec<ObjectId>,
    store: Store,
}
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.ids.push(object.id());
        self.store.accept(object)
    }
}

fn direct(
    case: &Case,
    reader: layerfs_overlay::CapturedReader,
    scope: IndexedOperationRecordScope,
    serial: u64,
    emitted: &mut Emitted,
) -> CapturedFileAttempt {
    let policy = ConstructionPolicy::frozen_default();
    Timing::disabled("direct-route", |timing| {
        Ok::<CapturedFileAttempt, ContentError>(CapturedFileEdits::construct_new(
            &case.workspace,
            &case.client,
            reader,
            serial,
            scope,
            None,
            policy,
            &policy.capacities(),
            emitted,
            timing.child("construct"),
        ))
    })
    .0
    .unwrap()
}

fn recorded(
    case: &Case,
    reader: layerfs_overlay::CapturedReader,
    scope: IndexedOperationRecordScope,
    serial: u64,
    emitted: &mut Emitted,
) -> CapturedFileAttempt {
    let prepared =
        CapturedFileEdits::prepare(&case.workspace, &case.client, reader, serial, scope).unwrap();
    let policy = ConstructionPolicy::frozen_default();
    Timing::disabled("record-route", |timing| {
        Ok::<CapturedFileAttempt, ContentError>(prepared.construct(
            policy,
            &policy.capacities(),
            emitted,
            timing.child("construct"),
        ))
    })
    .0
    .unwrap()
}

fn create(case: &Case, name: &str) -> u64 {
    case.mutate(Operation::Create {
        parent: 1,
        name: common::name(name),
        mode: 0o644,
    })
    .unwrap()
    .serial
}

#[test]
fn a_new_file_is_identical_on_the_direct_and_the_record_route() {
    let case = Case::new();
    let base_root = case
        .workspace
        .base()
        .unwrap()
        .inode(BASE_FILE)
        .unwrap()
        .value
        .content_root
        .to_bytes();
    let mut files = Vec::new();
    // Empty.
    files.push((create(&case, "empty"), Vec::new()));
    // Small.
    let small = create(&case, "small");
    case.write(small, 0, b"one small file");
    files.push((small, b"one small file".to_vec()));
    // Sparse: a hole, data, then a zero tail.
    let sparse = create(&case, "sparse");
    case.write(sparse, 70_000, b"after a hole");
    case.size(sparse, 200_000);
    let mut bytes = vec![0; 200_000];
    bytes[70_000..70_012].copy_from_slice(b"after a hole");
    files.push((sparse, bytes));
    // Dense and multi-chunk.
    let dense = create(&case, "dense");
    let bytes: Vec<u8> = (0..300_000_u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    for (at, window) in bytes.chunks(65_536).enumerate() {
        case.write(dense, (at * 65_536) as u64, window);
    }
    files.push((dense, bytes));
    // A piece claimed from committed content, between new bytes.
    let piece = create(&case, "piece");
    case.write(piece, 0, b"head-");
    let claimed = &case.bytes[1_000..71_000];
    case.mutate(Operation::Write {
        serial: piece,
        position: Position::At(5),
        data: WriteData::from(claimed).claimed(WriteClaims {
            source: base_root,
            after: None,
            ranges: vec![ClaimedRange {
                offset: 5,
                length: claimed.len() as u64,
                source_offset: 1_000,
            }],
        }),
    });
    let mut bytes = b"head-".to_vec();
    bytes.extend_from_slice(claimed);
    files.push((piece, bytes));

    let (reader, scope) = case.captured(500);
    let other = IndexedOperationRecordScope {
        owner: scope.owner,
        file_scope: 0,
    };
    for (index, (serial, expected)) in files.iter().enumerate() {
        let file_scope = 600 + 2 * index as u64;
        let mut old = Emitted {
            ids: Vec::new(),
            store: case.store.clone(),
        };
        let mut new = Emitted {
            ids: Vec::new(),
            store: case.store.clone(),
        };
        let by_records = recorded(
            &case,
            reader,
            IndexedOperationRecordScope {
                file_scope,
                ..scope
            },
            *serial,
            &mut old,
        );
        let direct_scope = IndexedOperationRecordScope {
            file_scope: file_scope + 1,
            ..scope
        };
        let by_direct = direct(&case, reader, direct_scope, *serial, &mut new);
        let (a, b) = (
            by_records.result.as_ref().unwrap(),
            by_direct.result.as_ref().unwrap(),
        );
        assert_eq!(a.root, b.root, "serial {serial}: root");
        assert_eq!(a.logical_len, b.logical_len, "serial {serial}: length");
        assert_eq!(a.logical_len, expected.len() as u64);
        assert_eq!(old.ids, new.ids, "serial {serial}: emitted objects");
        case.oracle(b.root, expected, None);
        // The direct route wrote, read and left no record in its scope.
        let left = case
            .client
            .operation_record_keys_after(direct_scope, 0x4346_0000, None)
            .unwrap()
            .value;
        assert!(left.is_empty(), "serial {serial}: records left");
        assert_eq!(by_direct.custody.work.normalized_edits, 0);
        assert!(by_direct.custody.failure.is_none());
        drop(by_records.custody);
        drop(by_direct.custody);
    }
    // A file the base holds is refused on the direct route, keeping custody.
    let mut ignored = Emitted {
        ids: Vec::new(),
        store: case.store.clone(),
    };
    let refused = direct(&case, reader, other, BASE_FILE, &mut ignored);
    assert!(refused.result.is_err());
    assert!(refused.custody.failure.is_some());
    assert!(ignored.ids.is_empty());
    case.release(refused.custody, false);
}
