use layerfs_core::content::persistence as file_codec;
use layerfs_core::cow::persistence as dir_codec;
use layerfs_core::delta::codec as delta_codec;
use layerfs_core::object::{decode_object, encode_object, Object};
use layerfs_core::{chunk_id, validate_identity, CanonicalName, CoreError, ObjectId};

fn canonical_mapping(inner: Vec<u8>) -> Vec<u8> {
    encode_object(&Object::bytes(inner).expect("test bytes")).expect("canonical bytes")
}

#[test]
fn file_mapping_round_trips_at_leaf_boundaries() {
    for count in [0_usize, 1, 63, 64, 65, 101, 256, 257] {
        let references = (0..count)
            .map(|index| {
                let bytes = [u8::try_from(index & 0xff).expect("byte")];
                file_codec::FileReference {
                    raw_id: chunk_id(&bytes),
                    raw_length: 1,
                    object_id: ObjectId::for_bytes(&bytes),
                }
            })
            .collect::<Vec<_>>();
        let inner = file_codec::encode_file_leaf(&references).expect("leaf");
        let canonical = canonical_mapping(inner);
        let payload =
            file_codec::decode_mapping(&canonical, file_codec::FILE_LEAF_TAG).expect("decode");
        assert_eq!(
            file_codec::parse_file_leaf(&payload).expect("parse"),
            references
        );
    }
}

#[test]
fn file_root_and_delta_preserve_ordered_identity() {
    let child = ObjectId::for_bytes(b"child");
    let root_inner = file_codec::encode_file_root(
        0,
        9,
        2,
        0,
        &[
            file_codec::FileChild {
                object_id: child,
                cumulative_end: 4,
            },
            file_codec::FileChild {
                object_id: child,
                cumulative_end: 9,
            },
        ],
    )
    .expect("root");
    let root = canonical_mapping(root_inner);
    let payload =
        file_codec::decode_mapping(&root, file_codec::FILE_ROOT_TAG).expect("root decode");
    let (total, references, level, children) =
        file_codec::parse_file_root(&payload).expect("root parse");
    assert_eq!((total, references, level), (9, 2, 0));
    assert_eq!(children.len(), 2);
    let delta_inner = delta_codec::encode_change(child, ObjectId::for_bytes(b"next"), 1, &[child])
        .expect("delta");
    let delta = canonical_mapping(delta_inner);
    assert_eq!(
        decode_object(&delta).expect("delta object").kind(),
        layerfs_core::ObjectKind::Bytes
    );
}

#[test]
fn malformed_mapping_and_identity_mismatch_are_rejected() {
    let mut inner = file_codec::encode_file_leaf(&[]).expect("leaf");
    inner[8..10].copy_from_slice(&2_u16.to_be_bytes());
    let canonical = canonical_mapping(inner);
    assert_eq!(
        file_codec::decode_mapping(&canonical, file_codec::FILE_LEAF_TAG),
        Err(CoreError::UnsupportedMappingVersion { version: 2 })
    );

    let valid = canonical_mapping(file_codec::encode_file_leaf(&[]).expect("leaf"));
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(matches!(
        decode_object(&trailing),
        Err(CoreError::TrailingBytes)
    ));
    assert_eq!(
        validate_identity(&valid, ObjectId::for_bytes(b"wrong")),
        Err(CoreError::IdentityMismatch)
    );
}

#[test]
fn directory_pages_and_wrappers_are_canonical() {
    let child = ObjectId::for_bytes(b"child");
    let entries = vec![
        layerfs_core::DirectoryEntry::new(
            CanonicalName::new("00000001").expect("name"),
            layerfs_core::ObjectReference::new(layerfs_core::ObjectKind::Bytes, child),
        ),
        layerfs_core::DirectoryEntry::new(
            CanonicalName::new("00000002").expect("name"),
            layerfs_core::ObjectReference::new(layerfs_core::ObjectKind::Bytes, child),
        ),
    ];
    let page = dir_codec::encode_directory_page(&entries).expect("page");
    let page_id = ObjectId::for_bytes(&page);
    let index_inner = dir_codec::encode_directory_index(
        2,
        &[dir_codec::DirectoryPageRef {
            count: 2,
            first_name: b"00000001".to_vec(),
            object_id: page_id,
        }],
    )
    .expect("index");
    let index = canonical_mapping(index_inner);
    let index_payload =
        file_codec::decode_mapping(&index, file_codec::DIR_INDEX_TAG).expect("index decode");
    assert_eq!(
        dir_codec::parse_directory_index(&index_payload)
            .expect("index parse")
            .len(),
        1
    );
    let metadata = canonical_mapping(dir_codec::encode_directory_metadata(0).expect("metadata"));
    let wrapper = dir_codec::encode_directory_wrapper(
        ObjectId::for_bytes(&metadata),
        ObjectId::for_bytes(&index),
    )
    .expect("wrapper");
    assert_eq!(
        decode_object(&wrapper).expect("wrapper decode").kind(),
        layerfs_core::ObjectKind::Directory
    );
}
