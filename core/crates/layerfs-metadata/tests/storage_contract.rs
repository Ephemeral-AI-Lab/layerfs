//! Live PostgreSQL uses the exact memory-engine metadata contract file.
#[path = "../../layerfs-storage/tests/support/metadata_contract.rs"]
mod contract;
mod support;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_metadata::PgMetadata;
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo},
    policy::StoragePolicy,
    port::*,
};
#[test]
fn every_metadata_unit_is_one_actual_wire_round_trip() {
    let store =
        PgMetadata::create(support::config("units"), StoragePolicy::frozen_default()).unwrap();
    contract::all_units(&store, &|| {
        let counts = store.diagnostics().unwrap();
        contract::Counts {
            submitted: counts.sync_messages,
            completed: counts.ready_for_query,
        }
    });
    let before = store.diagnostics().unwrap();
    let work = store.statement_work();
    assert_eq!(work.omitted, 0);
    assert_eq!(
        work.statements.iter().map(|row| row.calls).sum::<u64>(),
        before.operations
    );
    assert!(work
        .statements
        .iter()
        .all(|row| row.caller_ns >= row.queue_ns + row.driver_ns));
    assert_eq!(
        store.diagnostics().unwrap(),
        before,
        "observation must issue no SQL"
    );
    println!("DIAGNOSTIC pg-units {before:?} work={work:?}");
}
#[test]
fn two_connections_first_wins_without_a_registration_retry() {
    let config = support::config("race");
    let a = PgMetadata::create(config.clone(), StoragePolicy::frozen_default()).unwrap();
    let b = PgMetadata::open(config).unwrap();
    let id = ObjectId::for_bytes(b"race record");
    let first = a
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let second = b
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    assert_ne!(first, second);
    let make = |pack_id| {
        let body = vec![7; 64];
        let info = PackInfo {
            pack_id,
            domain: PackDomain::Metadata,
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        };
        Registration {
            packs: vec![RegisteredPack {
                info,
                body: Some(body),
            }],
            objects: vec![ObjectLocation {
                object_id: id,
                role: ObjectRole::DirectoryLeaf,
                canonical_length: 100,
                pack_id,
                group_number: 0,
                record_number: 0,
            }],
            ..Default::default()
        }
    };
    let (left, right) = std::thread::scope(|threads| {
        let left = threads.spawn(|| a.register(&make(first)).unwrap());
        let right = threads.spawn(|| b.register(&make(second)).unwrap());
        (left.join().unwrap(), right.join().unwrap())
    });
    assert_eq!(left.lost.len() + right.lost.len(), 1);
    let mut rows = Vec::new();
    a.locate(&[id], &mut rows).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].location.pack_id,
        if left.lost.is_empty() { first } else { second }
    );
    println!(
        "DIAGNOSTIC pg-race a={:?} b={:?}",
        a.diagnostics().unwrap(),
        b.diagnostics().unwrap()
    );
}
#[test]
fn malformed_input_has_no_wire_attempt_and_definite_refusal_is_atomic() {
    let config = support::config("refusal");
    let store = PgMetadata::create(config.clone(), StoragePolicy::frozen_default()).unwrap();
    let before = store.diagnostics().unwrap();
    assert_eq!(
        store.reserve(Reserve {
            packs: 9000,
            ordinals: 0
        }),
        Err(MetadataError::Malformed)
    );
    assert_eq!(store.diagnostics().unwrap(), before);
    assert!(matches!(
        PgMetadata::create(config, StoragePolicy::frozen_default()),
        Err(MetadataError::Refused { .. })
    ));
    let id = ObjectId::for_bytes(b"missing pack");
    let batch = Registration {
        objects: vec![ObjectLocation {
            object_id: id,
            role: ObjectRole::DirectoryLeaf,
            canonical_length: 100,
            pack_id: 99999,
            group_number: 0,
            record_number: 0,
        }],
        ..Default::default()
    };
    assert!(matches!(
        store.register(&batch),
        Err(MetadataError::Refused { .. })
    ));
    let mut rows = Vec::new();
    store.locate(&[id], &mut rows).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn bounded_registration_keeps_mixed_conflicts_in_input_order_and_rolls_back() {
    let config = support::config("bulk");
    let store = PgMetadata::create(config.clone(), StoragePolicy::frozen_default()).unwrap();
    let pack = store
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let body = vec![7; 64];
    store
        .register(&Registration {
            packs: vec![RegisteredPack {
                info: PackInfo {
                    pack_id: pack,
                    domain: PackDomain::Metadata,
                    key: ObjectKey::for_bytes(&body),
                    length: body.len(),
                },
                body: Some(body),
            }],
            ..Default::default()
        })
        .unwrap();
    let make = |n: u8| ObjectLocation {
        object_id: ObjectId::for_bytes(&[n]),
        role: ObjectRole::DirectoryLeaf,
        canonical_length: 100,
        pack_id: pack,
        group_number: 0,
        record_number: n as usize,
    };
    let first = vec![make(3), make(1)];
    store
        .register(&Registration {
            objects: first.clone(),
            ..Default::default()
        })
        .unwrap();
    let batch = Registration {
        objects: vec![make(3), make(2), make(1), make(4)],
        ..Default::default()
    };
    let before = store.diagnostics().unwrap();
    assert_eq!(
        store.register(&batch).unwrap().lost,
        vec![make(3).object_id, make(1).object_id]
    );
    assert_eq!(
        store.diagnostics().unwrap().sync_messages - before.sync_messages,
        1
    );
    let mut bad = make(5);
    bad.pack_id = 999999;
    assert!(matches!(
        store.register(&Registration {
            objects: vec![make(6), bad],
            ..Default::default()
        }),
        Err(MetadataError::Refused { .. })
    ));
    let mut rows = Vec::new();
    store
        .locate(&[make(5).object_id, make(6).object_id], &mut rows)
        .unwrap();
    assert!(rows.is_empty());
    let mut rows = Vec::new();
    store
        .locate(
            &[
                make(1).object_id,
                make(2).object_id,
                make(3).object_id,
                make(4).object_id,
            ],
            &mut rows,
        )
        .unwrap();
    assert_eq!(rows.len(), 4);
}
