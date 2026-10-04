//! Verified read carrier and retained C2 framing validation across the port.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{
    location::{PackDomain, PackInfo},
    port::{ObjectKey, PersistedPack, PersistenceError},
    Storage,
};
use std::sync::Arc;

#[test]
fn immutable_carrier_rejects_wrong_length_digest_and_modified_consumed_bytes() {
    let body = vec![7; 128];
    let info = PackInfo {
        pack_id: 1,
        domain: PackDomain::Payload,
        key: ObjectKey::for_bytes(&body),
        length: body.len(),
    };
    let verified = PersistedPack::authenticate(info, body.clone()).unwrap();
    assert_eq!(verified.info(), info);
    assert_eq!(verified.body(), body);
    assert_eq!(
        PersistedPack::authenticate(
            PackInfo {
                length: 129,
                ..info
            },
            body.clone()
        ),
        Err(PersistenceError::Malformed)
    );
    assert_eq!(
        PersistedPack::authenticate(
            PackInfo {
                key: ObjectKey::for_bytes(b"wrong"),
                ..info
            },
            body
        ),
        Err(PersistenceError::Malformed)
    );
    let (descriptor, mut consumed) = verified.into_parts();
    consumed[0] ^= 1;
    assert_eq!(
        PersistedPack::authenticate(descriptor, consumed),
        Err(PersistenceError::Malformed)
    );
}
#[test]
fn c2_still_refuses_hash_valid_pack_with_wrong_domain() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata.clone()).unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(b"payload").unwrap(),
    )
    .unwrap();
    let save = store.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    {
        let mut state = metadata.state.lock().unwrap();
        let pack_id = state.objects[&object.id()].pack_id;
        let pack = state.packs.get_mut(&pack_id).unwrap();
        pack.info.domain = match pack.info.domain {
            PackDomain::Payload => PackDomain::Metadata,
            PackDomain::Metadata => PackDomain::Payload,
        };
    }
    let reader_store = Storage::new(metadata).unwrap();
    assert!(reader_store
        .reader()
        .unwrap()
        .read_objects(&[object.id()])
        .is_err());
}
