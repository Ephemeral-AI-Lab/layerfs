use core::cmp::Ordering;

use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1};
use layerfs_storage::object::{
    decode_physical_object_v1, DiscardStrongEdgesV1, TypedPhysicalObjectIdV1,
};
use layerfs_storage::pack::{
    build_dense_pack_v1, validate_pack_v1, PackIndexEntryV1, PackIndexSpoolV1, PackObjectSourceV1,
    PackPortErrorV1, PackReadPortV1, PrivatePackPortV1, MAX_PACK_BYTES,
};
use layerfs_storage::profile::ProfileSpecV1;
use layerfs_storage::{CoreError, CoreResult};

mod support;

fn object(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(52 + payload.len());
    bytes.extend_from_slice(b"ELSOBJ01");
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.push(kind);
    bytes.push(0);
    bytes.extend_from_slice(ProfileSpecV1::frozen().id().as_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn typed_id(bytes: &[u8]) -> TypedPhysicalObjectIdV1 {
    decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)
        .unwrap()
        .physical_id()
        .unwrap()
}

struct VecObjectSource<'a> {
    bytes: &'a [Vec<u8>],
    ids: Vec<TypedPhysicalObjectIdV1>,
    reported_resident_bytes: Option<u64>,
    metadata_reads: u64,
    payload_bytes_read: u64,
    fail_reads: bool,
}

impl<'a> VecObjectSource<'a> {
    fn new(bytes: &'a [Vec<u8>]) -> Self {
        Self {
            bytes,
            ids: bytes.iter().map(|value| typed_id(value)).collect(),
            reported_resident_bytes: None,
            metadata_reads: 0,
            payload_bytes_read: 0,
            fail_reads: false,
        }
    }
}

impl PackObjectSourceV1 for VecObjectSource<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        if let Some(bytes) = self.reported_resident_bytes {
            return Ok(bytes);
        }
        u64::try_from(self.ids.capacity())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_mul(core::mem::size_of::<TypedPhysicalObjectIdV1>() as u64)
            .ok_or(CoreError::IntegerOverflow)
    }

    fn declared_object_count(&self) -> CoreResult<u32> {
        u32::try_from(self.bytes.len()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn object_id(&mut self, ordinal: u32) -> Result<TypedPhysicalObjectIdV1, PackPortErrorV1> {
        self.metadata_reads = self
            .metadata_reads
            .checked_add(1)
            .ok_or(PackPortErrorV1::Failure)?;
        self.ids
            .get(ordinal as usize)
            .copied()
            .ok_or(PackPortErrorV1::Failure)
    }

    fn object_len(&mut self, ordinal: u32) -> Result<u64, PackPortErrorV1> {
        self.metadata_reads = self
            .metadata_reads
            .checked_add(1)
            .ok_or(PackPortErrorV1::Failure)?;
        self.bytes
            .get(ordinal as usize)
            .ok_or(PackPortErrorV1::Failure)
            .and_then(|bytes| u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure))
    }

    fn read_object_exact_at(
        &mut self,
        ordinal: u32,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1> {
        if self.fail_reads {
            return Err(PackPortErrorV1::Failure);
        }
        let bytes = self
            .bytes
            .get(ordinal as usize)
            .ok_or(PackPortErrorV1::Failure)?;
        let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
        let end = start
            .checked_add(destination.len())
            .ok_or(PackPortErrorV1::Failure)?;
        destination.copy_from_slice(bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?);
        self.payload_bytes_read = self
            .payload_bytes_read
            .checked_add(destination.len() as u64)
            .ok_or(PackPortErrorV1::Failure)?;
        Ok(())
    }
}

#[derive(Default)]
struct VecPack {
    bytes: Vec<u8>,
    reported_resident_bytes: Option<u64>,
    expected_len: u64,
    sealed: bool,
    aborted: bool,
    begins: u64,
    len_calls: u64,
    fail_after: Option<usize>,
    fail_reads: bool,
}

impl PackReadPortV1 for VecPack {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(self.reported_resident_bytes.unwrap_or(0))
    }

    fn len(&mut self) -> Result<u64, PackPortErrorV1> {
        self.len_calls = self
            .len_calls
            .checked_add(1)
            .ok_or(PackPortErrorV1::Failure)?;
        u64::try_from(self.bytes.len()).map_err(|_| PackPortErrorV1::Failure)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1> {
        if self.fail_reads {
            return Err(PackPortErrorV1::Failure);
        }
        let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
        let end = start
            .checked_add(destination.len())
            .ok_or(PackPortErrorV1::Failure)?;
        let source = self.bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?;
        destination.copy_from_slice(source);
        Ok(())
    }
}

impl PrivatePackPortV1 for VecPack {
    fn begin_private(&mut self, exact_len: u64) -> Result<(), PackPortErrorV1> {
        if exact_len > MAX_PACK_BYTES || self.begins != 0 {
            return Err(PackPortErrorV1::Failure);
        }
        self.expected_len = exact_len;
        self.begins += 1;
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), PackPortErrorV1> {
        if self
            .fail_after
            .is_some_and(|limit| self.bytes.len() >= limit)
        {
            return Err(PackPortErrorV1::Failure);
        }
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(PackPortErrorV1::Failure)?;
        if u64::try_from(next).map_err(|_| PackPortErrorV1::Failure)? > self.expected_len {
            return Err(PackPortErrorV1::Failure);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn seal_private(
        &mut self,
        _id: layerfs_storage::identity::PackIdV1,
    ) -> Result<(), PackPortErrorV1> {
        if u64::try_from(self.bytes.len()).map_err(|_| PackPortErrorV1::Failure)?
            != self.expected_len
        {
            return Err(PackPortErrorV1::Failure);
        }
        self.sealed = true;
        Ok(())
    }

    fn abort_private(&mut self) {
        self.aborted = true;
        self.sealed = false;
    }
}

#[derive(Default)]
struct VecSpool {
    entries: Vec<PackIndexEntryV1>,
    cursor: usize,
    maximum: usize,
    peak: usize,
    aborted: bool,
    reported_resident_bytes: Option<u64>,
}

impl PackIndexSpoolV1 for VecSpool {
    fn resident_memory_bound_bytes(&self, maximum_entries: u32) -> CoreResult<u64> {
        if let Some(bytes) = self.reported_resident_bytes {
            return Ok(bytes);
        }
        u64::from(maximum_entries)
            .checked_mul(core::mem::size_of::<PackIndexEntryV1>() as u64)
            .ok_or(CoreError::IntegerOverflow)
    }

    fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1> {
        self.entries.clear();
        self.cursor = 0;
        self.maximum = maximum_entries as usize;
        self.aborted = false;
        Ok(())
    }

    fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1> {
        if self.entries.len() >= self.maximum {
            return Err(PackPortErrorV1::Failure);
        }
        self.entries.push(entry);
        self.peak = self.peak.max(self.entries.len());
        Ok(())
    }

    fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1> {
        self.entries.sort_by(PackIndexEntryV1::compare_key);
        Ok(())
    }

    fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1> {
        self.entries.sort_by(PackIndexEntryV1::compare_offset);
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), PackPortErrorV1> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
        let next = self.entries.get(self.cursor).copied();
        self.cursor = self
            .cursor
            .checked_add(usize::from(next.is_some()))
            .ok_or(PackPortErrorV1::Failure)?;
        Ok(next)
    }

    fn abort(&mut self) {
        self.aborted = true;
        self.entries.clear();
    }
}

fn build(
    bytes: &[Vec<u8>],
) -> CoreResult<(
    VecPack,
    VecSpool,
    OperationCountersV1,
    layerfs_storage::pack::SealedPackV1,
)> {
    let mut source = VecObjectSource::new(bytes);
    let mut pack = VecPack::default();
    let mut spool = VecSpool::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let sealed = build_dense_pack_v1(
        &mut source,
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut scratch,
    )?;
    assert_eq!(
        source.payload_bytes_read,
        bytes.iter().map(|value| value.len() as u64).sum::<u64>(),
        "each source object is copied into the private pack exactly once"
    );
    assert_eq!(ledger.admitted_slots(), 0);
    Ok((pack, spool, counters, sealed))
}

fn be_u64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes(bytes.try_into().unwrap())
}

fn reseal(pack: &mut [u8]) {
    let checksum_at = pack.len() - 32;
    let mut frame = [0_u8; 20];
    frame[..8].copy_from_slice(b"ELSHASH1");
    frame[8] = 0x20;
    frame[12..].copy_from_slice(&(checksum_at as u64).to_be_bytes());
    let mut hasher = blake3::Hasher::new();
    hasher.update(&frame);
    hasher.update(&pack[..checksum_at]);
    let checksum = *hasher.finalize().as_bytes();
    pack[checksum_at..].copy_from_slice(&checksum);
}

fn framed_digest(tag: u8, bytes: &[u8]) -> [u8; 32] {
    let mut frame = [0_u8; 20];
    frame[..8].copy_from_slice(b"ELSHASH1");
    frame[8] = tag;
    frame[12..].copy_from_slice(&(bytes.len() as u64).to_be_bytes());
    let mut hasher = blake3::Hasher::new();
    hasher.update(&frame);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn independently_validate(pack: &mut VecPack) -> CoreResult<()> {
    let mut spool = VecSpool::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    validate_pack_v1(
        pack,
        &mut spool,
        &mut scratch,
        10_000,
        &ledger,
        &mut counters,
    )
    .map(|_| ())
}

#[test]
fn minimal_pack_is_exact_and_sealed_only_after_independent_validation() {
    let objects = [object(5, &[0])];
    let (pack, spool, counters, sealed) = build(&objects).unwrap();
    assert!(pack.sealed);
    assert!(!pack.aborted);
    assert_eq!(pack.bytes.len(), 288);
    assert_eq!(sealed.pack_len(), 288);
    assert_eq!(sealed.record_count(), 1);
    assert_eq!(sealed.index_offset(), 128);
    assert_eq!(
        sealed.id().as_bytes(),
        &support::expected("90bf9bf15f2d23614bc3fbd3807ba4ec114da4b882b707fadcc1020651097471")
    );
    assert_eq!(&pack.bytes[..8], b"ELSPACK1");
    assert_eq!(&pack.bytes[128 + 80..128 + 88], b"ELSPEND1");
    assert_eq!(spool.peak, 1);
    assert_eq!(counters.pack_entries, 1);
    assert_eq!(counters.pack_bytes, 288);
    assert_eq!(counters.bytes_written, 288);
    assert_eq!(counters.memory_high_water, 12_582_912);
}

#[test]
fn mixed_kind_records_keep_discovery_order_and_index_has_strict_typed_order() {
    let empty_root = object(2, &[1, 0x10, 0, 0, 0, 0, 0, 0, 0]);
    let symlink = object(4, &[0, 0, 0, 1, b'x']);
    let chunk = object(5, &[7]);
    let objects = [chunk, symlink, empty_root];
    let physical_ids: Vec<_> = objects.iter().map(|value| typed_id(value)).collect();
    let (pack, _, _, sealed) = build(&objects).unwrap();
    let mut offset = 64_usize;
    for (object, id) in objects.iter().zip(physical_ids) {
        assert_eq!(
            u32::from_be_bytes(pack.bytes[offset..offset + 4].try_into().unwrap()) as usize,
            object.len()
        );
        assert_eq!(
            typed_id(&pack.bytes[offset + 4..offset + 4 + object.len()]),
            id
        );
        offset += (4 + object.len() + 7) & !7;
    }
    assert_eq!(offset as u64, sealed.index_offset());
    let mut previous: Option<(u8, [u8; 32])> = None;
    for entry in pack.bytes[offset..offset + objects.len() * 80].chunks_exact(80) {
        let key = (entry[0], <[u8; 32]>::try_from(&entry[4..36]).unwrap());
        assert!(previous.is_none_or(|left| left.cmp(&key) == Ordering::Less));
        previous = Some(key);
    }
}

#[test]
fn large_dense_pack_uses_one_bounded_window_and_metadata_only_spool() {
    let objects: Vec<_> = (0_u16..10_000)
        .map(|value| object(5, &value.to_be_bytes()))
        .collect();
    let (pack, spool, counters, sealed) = build(&objects).unwrap();
    assert!(pack.sealed);
    assert_eq!(sealed.record_count(), 10_000);
    assert_eq!(spool.peak, 10_000);
    assert_eq!(counters.pack_entries, 10_000);
    assert_eq!(counters.pack_bytes, pack.bytes.len() as u64);
    assert_eq!(counters.memory_high_water, 12_582_912);
    assert_eq!(
        counters.bytes_read,
        objects.iter().map(|value| value.len() as u64).sum::<u64>()
            + counters.pack_bytes * 2
            - 32,
        "construction reads each source once, then performs an independent full-pack hash pass and exact structural pass"
    );
}

#[test]
fn oversized_index_residency_is_refused_before_pack_output() {
    let objects = [object(5, &[0])];
    let mut source = VecObjectSource::new(&objects);
    let mut pack = VecPack::default();
    let mut spool = VecSpool {
        reported_resident_bytes: Some(layerfs_storage::limits::OPERATION_SLOT_BYTES),
        ..VecSpool::default()
    };
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    assert_eq!(
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::ResourceRefused)
    );
    assert_eq!(pack.begins, 0);
    assert!(pack.bytes.is_empty());
    assert_eq!(source.metadata_reads, 0);
    assert_eq!(source.payload_bytes_read, 0);
}

#[test]
fn oversized_source_residency_is_refused_before_payload_read_or_pack_output() {
    let objects = [object(5, &[0])];
    let mut source = VecObjectSource::new(&objects);
    source.reported_resident_bytes = Some(layerfs_storage::limits::OPERATION_SLOT_BYTES);
    let mut pack = VecPack::default();
    let mut spool = VecSpool::default();
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    assert_eq!(
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::ResourceRefused)
    );
    assert_eq!(source.payload_bytes_read, 0);
    assert_eq!(source.metadata_reads, 0);
    assert_eq!(pack.begins, 0);
    assert!(pack.bytes.is_empty());
}

#[test]
fn oversized_pack_port_residency_is_refused_before_source_preflight_or_output() {
    let objects = [object(5, &[0])];
    let mut source = VecObjectSource::new(&objects);
    let mut pack = VecPack {
        reported_resident_bytes: Some(layerfs_storage::limits::OPERATION_SLOT_BYTES),
        ..VecPack::default()
    };
    let mut spool = VecSpool::default();
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    assert_eq!(
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::ResourceRefused)
    );
    assert_eq!(source.metadata_reads, 0);
    assert_eq!(source.payload_bytes_read, 0);
    assert_eq!(pack.begins, 0);
    assert_eq!(pack.len_calls, 0);
    assert!(pack.bytes.is_empty());
}

#[test]
fn validation_charges_pack_port_residency_before_length_or_payload_reads() {
    let (valid, _, _, _) = build(&[object(5, &[0])]).unwrap();
    let mut pack = VecPack {
        bytes: valid.bytes,
        reported_resident_bytes: Some(layerfs_storage::limits::OPERATION_SLOT_BYTES),
        ..VecPack::default()
    };
    let mut spool = VecSpool::default();
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    assert_eq!(
        validate_pack_v1(
            &mut pack,
            &mut spool,
            &mut scratch,
            10_000,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut counters,
        ),
        Err(CoreError::ResourceRefused)
    );
    assert_eq!(pack.len_calls, 0);
    assert_eq!(counters.bytes_read, 0);
}

#[test]
fn duplicate_key_and_sink_refusal_abort_without_sealing() {
    let bytes = object(5, &[1]);
    let duplicate_objects = [bytes.clone(), bytes.clone()];
    let mut duplicate_source = VecObjectSource::new(&duplicate_objects);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = VecPack::default();
    let mut spool = VecSpool::default();
    assert_eq!(
        build_dense_pack_v1(
            &mut duplicate_source,
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::NonCanonicalOrder)
    );
    assert!(pack.aborted);
    assert!(!pack.sealed);

    let mut pack = VecPack {
        fail_after: Some(64),
        ..VecPack::default()
    };
    let mut spool = VecSpool::default();
    let one_object = [bytes];
    let mut one_source = VecObjectSource::new(&one_object);
    assert_eq!(
        build_dense_pack_v1(
            &mut one_source,
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::SinkRefused)
    );
    assert!(pack.aborted);
    assert!(!pack.sealed);
    assert_eq!(ledger.admitted_slots(), 0);
}

#[test]
fn hostile_index_record_seal_truncation_overlap_and_trailing_bytes_fail_closed() {
    let objects = [object(5, &[0]), object(5, &[1])];
    let (valid, _, _, _) = build(&objects).unwrap();
    let index_offset = be_u64(&valid.bytes[56..64]) as usize;

    let mut duplicate = VecPack {
        bytes: valid.bytes.clone(),
        ..VecPack::default()
    };
    duplicate
        .bytes
        .copy_within(index_offset + 4..index_offset + 36, index_offset + 84);
    reseal(&mut duplicate.bytes);
    assert_eq!(
        independently_validate(&mut duplicate),
        Err(CoreError::PackInvalid)
    );

    let mut overlap = VecPack {
        bytes: valid.bytes.clone(),
        ..VecPack::default()
    };
    overlap.bytes[index_offset + 80 + 36..index_offset + 80 + 44]
        .copy_from_slice(&64_u64.to_be_bytes());
    reseal(&mut overlap.bytes);
    assert_eq!(
        independently_validate(&mut overlap),
        Err(CoreError::PackInvalid)
    );

    let mut bad_object_checksum = VecPack {
        bytes: valid.bytes.clone(),
        ..VecPack::default()
    };
    bad_object_checksum.bytes[index_offset + 48] ^= 1;
    reseal(&mut bad_object_checksum.bytes);
    assert_eq!(
        independently_validate(&mut bad_object_checksum),
        Err(CoreError::PackInvalid)
    );

    let mut bad_pack_checksum = VecPack {
        bytes: valid.bytes.clone(),
        ..VecPack::default()
    };
    let last = bad_pack_checksum.bytes.len() - 1;
    bad_pack_checksum.bytes[last] ^= 1;
    assert_eq!(
        independently_validate(&mut bad_pack_checksum),
        Err(CoreError::PackInvalid)
    );

    let mut truncated = VecPack {
        bytes: valid.bytes[..valid.bytes.len() - 1].to_vec(),
        ..VecPack::default()
    };
    assert_eq!(
        independently_validate(&mut truncated),
        Err(CoreError::PackInvalid)
    );

    let mut trailing = VecPack {
        bytes: valid.bytes.clone(),
        ..VecPack::default()
    };
    trailing.bytes.push(0);
    assert_eq!(
        independently_validate(&mut trailing),
        Err(CoreError::PackInvalid)
    );
}

#[test]
fn independent_validation_reparses_canonical_payload_not_just_consistent_hashes() {
    let mut leaf = vec![2, 0, 0, 2];
    for (name, id) in [(b'a', [0x11; 32]), (b'b', [0x12; 32])] {
        leaf.extend_from_slice(&1_u16.to_be_bytes());
        leaf.push(name);
        leaf.push(1);
        leaf.extend_from_slice(&id);
    }
    let (valid, _, _, _) = build(&[object(2, &leaf)]).unwrap();
    let mut malformed = VecPack {
        bytes: valid.bytes,
        ..VecPack::default()
    };

    let object_offset = 64 + 4;
    let object_len = u32::from_be_bytes(malformed.bytes[64..68].try_into().unwrap()) as usize;
    let second_name = object_offset + 52 + 42;
    malformed.bytes[second_name] = b'a';
    let object_bytes = &malformed.bytes[object_offset..object_offset + object_len];
    let object_id = framed_digest(0x12, object_bytes);
    let object_checksum = framed_digest(0x21, object_bytes);
    let index_offset = be_u64(&malformed.bytes[56..64]) as usize;
    malformed.bytes[index_offset + 4..index_offset + 36].copy_from_slice(&object_id);
    malformed.bytes[index_offset + 48..index_offset + 80].copy_from_slice(&object_checksum);
    reseal(&mut malformed.bytes);

    assert_eq!(
        independently_validate(&mut malformed),
        Err(CoreError::PackInvalid),
        "self-consistent hashes cannot authenticate non-canonical payload bytes"
    );
}

#[test]
fn empty_pack_is_refused_before_output_or_resource_reservation() {
    let empty: [Vec<u8>; 0] = [];
    let mut source = VecObjectSource::new(&empty);
    let mut pack = VecPack::default();
    let mut spool = VecSpool::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    assert_eq!(
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::CountCap)
    );
    assert_eq!(pack.begins, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(ledger.high_water_bytes(), 8_388_608);
}

#[test]
fn pack_read_failures_remain_source_failures() {
    let (mut pack, _, _, _) = build(&[object(5, &[0])]).unwrap();
    pack.fail_reads = true;
    assert_eq!(
        independently_validate(&mut pack),
        Err(CoreError::SourceFailure)
    );
}
