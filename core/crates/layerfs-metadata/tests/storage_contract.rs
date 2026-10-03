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
    println!("DIAGNOSTIC pg-units {:?}", store.diagnostics().unwrap());
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
