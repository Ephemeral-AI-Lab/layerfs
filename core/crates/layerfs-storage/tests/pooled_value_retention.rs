//! A hot authenticated value group survives unrelated bounded cache overflow.
use layerfs_content::{
    inode_leaf::{encode_inode_value, InodeKind, InodeValue},
    ObjectId,
};
use layerfs_storage::{
    encoding::{
        codec::DecompressionWorkspace,
        pool::{value_group, PoolReader},
        CompressionWorkspace,
    },
    location::{ObjectLocation, SignatureRow, ValueGroupRow},
    pack::{assemble, layout::PackLane},
    policy::{StorageCapacities, StoragePolicy, POOLED_VALUE_CACHE_BYTES, VALUES_PER_GROUP},
    source::Source,
    StorageError, StorageResult,
};
struct Input {
    packs: Vec<Vec<u8>>,
}
impl Source for Input {
    fn location(&self, _: ObjectId, _: i64) -> StorageResult<Option<ObjectLocation>> {
        Err(StorageError::Integrity("unexpected location"))
    }
    fn pack_bytes(&self, id: i64) -> StorageResult<Vec<u8>> {
        Ok(self.packs[(id - 1) as usize].clone())
    }
    fn value_group(&self, _: u32) -> StorageResult<Option<ValueGroupRow>> {
        Err(StorageError::Integrity("unexpected catalogue"))
    }
    fn value_groups(
        &self,
        _: Option<u32>,
        _: &mut dyn FnMut(ValueGroupRow) -> StorageResult<()>,
    ) -> StorageResult<()> {
        Err(StorageError::Integrity("unexpected catalogue scan"))
    }
    fn window_start(&self) -> StorageResult<u32> {
        Err(StorageError::Integrity("unexpected window"))
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        Err(StorageError::Integrity("unexpected signatures"))
    }
    fn write_signatures(&self, _: &[SignatureRow]) -> StorageResult<usize> {
        Err(StorageError::Integrity("unexpected write"))
    }
}
#[test]
fn a_hot_value_group_survives_capacity_eviction_without_extra_decoding() {
    let mut encode = CompressionWorkspace::new().unwrap();
    let mut input = Input { packs: Vec::new() };
    let mut rows = Vec::new();
    let mut expected = Vec::new();
    let fit = POOLED_VALUE_CACHE_BYTES / (VALUES_PER_GROUP * 73);
    for group in 0..=fit {
        let first = (group * VALUES_PER_GROUP + 1) as u32;
        let raw: Vec<_> = (0..VALUES_PER_GROUP)
            .map(|offset| {
                encode_inode_value(InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(&(first + offset as u32).to_be_bytes()),
                    metadata_root: ObjectId::for_bytes(b"metadata"),
                })
            })
            .collect();
        expected.push(raw[0]);
        let canonical: Vec<_> = raw
            .iter()
            .map(|value| value_group::canonical_value(value).unwrap())
            .collect();
        let built = value_group::build(&canonical, &mut encode).unwrap();
        input
            .packs
            .push(assemble(PackLane::PooledMetadata, &[built.group]).unwrap());
        rows.push(ValueGroupRow {
            first_ordinal: first,
            count: VALUES_PER_GROUP,
            pack_id: group as i64 + 1,
            group_number: 0,
            digest: built.digest,
        });
    }
    let capacities = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    let mut decode = DecompressionWorkspace::new().unwrap();
    let mut reader = PoolReader::new();
    assert_eq!(
        reader
            .group_value(&input, &capacities, i64::MAX, &mut decode, &rows[0], 1)
            .unwrap(),
        expected[0]
    );
    for number in 1..rows.len() {
        let row = &rows[number];
        assert_eq!(
            reader
                .group_value(
                    &input,
                    &capacities,
                    i64::MAX,
                    &mut decode,
                    row,
                    row.first_ordinal
                )
                .unwrap(),
            expected[number]
        );
        assert_eq!(
            reader
                .group_value(&input, &capacities, i64::MAX, &mut decode, &rows[0], 1)
                .unwrap(),
            expected[0]
        );
        assert!(reader.retained_bytes() <= POOLED_VALUE_CACHE_BYTES);
    }
    assert_eq!(reader.counters().value_group_decodes, rows.len() as u64);
    let counts = reader.counters();
    assert_eq!(counts.value_group_cache_evictions, 1);
    assert_eq!(
        counts.value_group_evicted_bytes,
        (VALUES_PER_GROUP * 73) as u64
    );
    assert_eq!(counts.value_group_cache_hits, fit as u64);
    assert_eq!(counts.since(counts).value_group_cache_evictions, 0);
    let mut total = layerfs_storage::encoding::pool::PoolReadCounters::new();
    total.accumulate(counts);
    assert_eq!(total, counts);
    // Retained hits still cannot bypass the caller's visibility ceiling.
    assert!(reader
        .group_value(&input, &capacities, 0, &mut decode, &rows[0], 1)
        .is_err());
}
