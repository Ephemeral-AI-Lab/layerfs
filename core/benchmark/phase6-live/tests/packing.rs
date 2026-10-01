use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};
use layerfs_storage::{
    encoding::{decode_canonical, CompressionWorkspace, DecompressionWorkspace, GroupCache},
    sqlite::lookup::ObjectLocation,
    StorageCapacities,
};
use phase6_live_probe::{
    objects::{Locator, ROLES},
    packing,
    pending::Pending,
    wire::{self, Bytes},
};
fn object(n: u32, size: usize) -> FinalizedObject {
    let mut payload = vec![0x41; size];
    payload[..4].copy_from_slice(&n.to_be_bytes());
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&payload).unwrap(),
    )
    .unwrap()
}
#[test]
fn multi_record_pack_reconstructs_exact_input_objects() {
    let objects: Vec<_> = (0..40).map(|n| object(n, 4096)).collect();
    let expected: Vec<_> = objects
        .iter()
        .map(|o| (o.id(), o.canonical().to_vec()))
        .collect();
    let packs = packing::build(objects, &mut CompressionWorkspace::new().unwrap()).unwrap();
    assert!(packs.len() < 40);
    let mut decoded = std::collections::BTreeMap::new();
    for pack in packs {
        let mut decoder = DecompressionWorkspace::new().unwrap();
        let mut cache = GroupCache::new();
        for row in pack.rows {
            let location = ObjectLocation {
                object_id: row.id,
                role: row.role,
                canonical_length: row.length,
                pack_id: 1,
                group_number: row.group as usize,
                record_number: row.record as usize,
            };
            let bytes = decode_canonical(
                &pack.bytes,
                &location,
                &StorageCapacities::from_policy(Default::default()).unwrap(),
                None,
                &mut decoder,
                &mut cache,
                &mut 0,
            )
            .unwrap();
            decoded.insert(row.id, bytes);
        }
    }
    for (id, bytes) in expected {
        assert_eq!(decoded.remove(&id).unwrap(), bytes);
    }
    assert!(decoded.is_empty());
}
#[test]
fn pending_count_and_byte_refusal_preserves_accepted_objects() {
    let mut p = Pending::default();
    for n in 0..512 {
        assert!(p.push(object(n, 8)).unwrap());
    }
    assert!(p.push(object(512, 8)).is_err());
    assert_eq!(p.objects.len(), 512);
    let before = p.bytes;
    assert!(!p.push(object(1, 8)).unwrap());
    assert_eq!(p.bytes, before);
    let mut q = Pending::default();
    for n in 0..31 {
        q.push(object(n, 128 * 1024)).unwrap();
    }
    assert!(q.push(object(31, 128 * 1024)).is_err());
    assert_eq!(q.objects.len(), 31);
    assert!(q.peak_bytes <= phase6_live_probe::pending::BYTES);
}
#[test]
fn packed_locator_codec_has_exact_fields_and_eof() {
    let row = Locator {
        id: object(1, 8).id(),
        role: ROLES[0],
        length: 123,
        pack: [0x42; 32],
        group: 17,
        record: 91,
        pack_id: 99,
    };
    let mut bytes = Vec::new();
    wire::locator(&mut bytes, &row);
    assert_eq!(bytes.len(), 89);
    let mut b = Bytes::new(&bytes);
    let decoded = wire::read_locator(&mut b).unwrap();
    b.done().unwrap();
    assert_eq!(decoded.id, row.id);
    assert_eq!(
        (decoded.group, decoded.record, decoded.pack_id),
        (17, 91, 99)
    );
    bytes.push(0);
    let mut b = Bytes::new(&bytes);
    wire::read_locator(&mut b).unwrap();
    assert!(b.done().is_err());
}

#[test]
fn native_pack_group_ceiling_seals_without_losing_objects() {
    let objects: Vec<_> = (0u32..300)
        .map(|n| {
            let mut data = vec![b'A'; 100];
            data[..4].copy_from_slice(&n.to_be_bytes());
            FinalizedObject::new(
                ObjectRole::Chunk,
                layerfs_content::file::mapping::encode_chunk_object(&data).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let packs = packing::build(objects, &mut CompressionWorkspace::new().unwrap()).unwrap();
    assert_eq!(packs.iter().map(|p| p.rows.len()).sum::<usize>(), 300);
    assert!(packs.len() >= 2);
    for p in packs {
        let header = layerfs_storage::pack::layout::parse_header(&p.bytes).unwrap();
        assert!(header.group_count <= header.lane.group_count_limit());
        assert!(p
            .rows
            .iter()
            .all(|r| (r.group as usize) < header.lane.group_count_limit()));
    }
}
