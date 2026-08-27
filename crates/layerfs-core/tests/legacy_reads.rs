use layerfs_core::legacy::{
    decode_mapping, read_mapping, FileReferenceV1, FileReferenceV2, LegacyMapping,
};
use layerfs_core::object::access::ObjectRead;
use layerfs_core::{encode_bytes_object, CoreError, CoreResult, ObjectId};
use std::collections::BTreeMap;

fn mapping(version: u16, tag: u8, body: &[u8]) -> Vec<u8> {
    let mut value = Vec::new();
    value.extend_from_slice(b"LFS4MAP\0");
    value.extend_from_slice(&version.to_be_bytes());
    value.push(tag);
    value.extend_from_slice(body);
    encode_bytes_object(&value).unwrap()
}

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

impl ObjectRead for MemoryStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }
}

#[test]
fn retained_v1_and_v2_mappings_are_strict_read_only_compatibility() {
    let raw_id = ObjectId::for_bytes(b"raw");
    let object_id = ObjectId::for_bytes(b"canonical");
    let mut v1_body = 1_u32.to_be_bytes().to_vec();
    v1_body.extend_from_slice(raw_id.as_bytes());
    v1_body.extend_from_slice(&3_u32.to_be_bytes());
    v1_body.extend_from_slice(object_id.as_bytes());
    let v1 = mapping(1, 2, &v1_body);
    assert_eq!(
        decode_mapping(&v1).unwrap(),
        LegacyMapping::FileLeafV1(vec![FileReferenceV1 {
            raw_id,
            raw_length: 3,
            object_id,
        }])
    );

    let mut v2_body = 1_u32.to_be_bytes().to_vec();
    v2_body.extend_from_slice(&3_u32.to_be_bytes());
    v2_body.extend_from_slice(object_id.as_bytes());
    let v2 = mapping(2, 2, &v2_body);
    assert_eq!(
        decode_mapping(&v2).unwrap(),
        LegacyMapping::FileLeafV2(vec![FileReferenceV2 {
            raw_length: 3,
            object_id,
        }])
    );

    let mut store = MemoryStore::default();
    let id = ObjectId::for_bytes(&v2);
    store.0.insert(id, v2.clone());
    assert_eq!(
        read_mapping(&store, id).unwrap(),
        decode_mapping(&v2).unwrap()
    );
    let mut corrupt = v2;
    *corrupt.last_mut().unwrap() ^= 1;
    store.0.insert(id, corrupt);
    assert_eq!(read_mapping(&store, id), Err(CoreError::IdentityMismatch));

    let unsupported = mapping(3, 2, &v2_body);
    assert!(matches!(
        decode_mapping(&unsupported),
        Err(CoreError::UnsupportedMappingVersion { version: 3 })
    ));
    let mut trailing = v1_body;
    trailing.push(0);
    assert_eq!(
        decode_mapping(&mapping(1, 2, &trailing)),
        Err(CoreError::TrailingBytes)
    );
}
