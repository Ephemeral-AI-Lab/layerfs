//! Shared external contract: the same valid units for memory and PostgreSQL.
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo, SignatureRow, ValueGroupRow},
    policy::StoragePolicy,
    port::*,
};
#[derive(Clone, Copy, Debug)]
pub struct Counts {
    pub submitted: u64,
    pub completed: u64,
}
fn unit<T>(counts: &dyn Fn() -> Counts, call: impl FnOnce() -> Result<T, MetadataError>) -> T {
    let before = counts();
    let result = call().expect("metadata unit");
    let after = counts();
    assert_eq!(after.submitted - before.submitted, 1);
    assert_eq!(after.completed - before.completed, 1);
    result
}
pub fn all_units(store: &dyn MetadataStore, counts: &dyn Fn() -> Counts) {
    assert_eq!(
        unit(counts, || store.policy()).validated().unwrap(),
        StoragePolicy::frozen_default()
    );
    let allocation = unit(counts, || {
        store.reserve(Reserve {
            packs: 2,
            ordinals: 8,
        })
    });
    assert!(allocation.first_pack_id > 0);
    assert!(allocation.first_ordinal > 0);
    let body = vec![7; 64];
    let metadata = PackInfo {
        pack_id: allocation.first_pack_id,
        domain: PackDomain::Metadata,
        key: ObjectKey::for_bytes(&body),
        length: body.len(),
    };
    let payload = PackInfo {
        pack_id: allocation.first_pack_id + 1,
        domain: PackDomain::Payload,
        key: ObjectKey::for_bytes(b"opaque payload descriptor"),
        length: 64,
    };
    let id = ObjectId::for_bytes(b"canonical test record");
    let location = ObjectLocation {
        object_id: id,
        role: ObjectRole::DirectoryLeaf,
        canonical_length: 100,
        pack_id: metadata.pack_id,
        group_number: 0,
        record_number: 0,
    };
    let ordinal = allocation.first_ordinal;
    let groups = vec![
        ValueGroupRow {
            first_ordinal: ordinal,
            count: 2,
            pack_id: metadata.pack_id,
            group_number: 0,
            digest: ObjectId::for_bytes(b"group zero"),
        },
        ValueGroupRow {
            first_ordinal: ordinal + 2,
            count: 1,
            pack_id: metadata.pack_id,
            group_number: 1,
            digest: ObjectId::for_bytes(b"group one"),
        },
    ];
    let signature = SignatureRow {
        slot: 0,
        stamp: 1,
        object_id: id,
        signature: [3; 32],
    };
    let batch = Registration {
        packs: vec![
            RegisteredPack {
                info: metadata,
                body: Some(body.clone()),
            },
            RegisteredPack {
                info: payload,
                body: None,
            },
        ],
        objects: vec![location],
        value_groups: groups.clone(),
        signatures: vec![signature],
        ..Default::default()
    };
    assert!(unit(counts, || store.register(&batch)).lost.is_empty());
    let mut found = Vec::new();
    unit(counts, || {
        store.locate(&[id, ObjectId::for_bytes(b"absent")], &mut found)
    });
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].location, location);
    assert_eq!(found[0].pack, metadata);
    let mut packs = Vec::new();
    unit(counts, || store.read_packs(&[metadata.pack_id], &mut packs));
    assert_eq!(
        packs,
        vec![MetadataPack {
            info: metadata,
            body
        }]
    );
    let found = unit(counts, || {
        store.value_groups(ValueGroupQuery::Ordinals(&[
            ordinal,
            ordinal + 1,
            ordinal + 2,
            ordinal + 7,
        ]))
    });
    assert_eq!(found.rows, groups);
    assert_eq!(found.window_start, 1);
    assert_eq!(found.next, None);
    let page = unit(counts, || {
        store.value_groups(ValueGroupQuery::Page {
            from: ordinal,
            limit: 1,
        })
    });
    assert_eq!(page.rows, vec![groups[0]]);
    assert_eq!(page.next, Some(ordinal + 2));
    let empty = unit(counts, || {
        store.value_groups(ValueGroupQuery::Page { from: 1, limit: 0 })
    });
    assert!(empty.rows.is_empty());
    assert_eq!(empty.next, None);
    let mut signatures = Vec::new();
    unit(counts, || store.signatures(&mut signatures));
    assert_eq!(signatures, vec![signature]);
    let release = Registration {
        release_ordinals: Some((ordinal + 3, 5)),
        ..Default::default()
    };
    unit(counts, || store.register(&release));
    let next = unit(counts, || {
        store.reserve(Reserve {
            packs: 0,
            ordinals: 1,
        })
    });
    assert_eq!(next.first_ordinal, ordinal + 3);
}
