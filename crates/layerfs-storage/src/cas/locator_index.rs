//! Bounded file-backed operation-wide locator lookup.
//!
//! This is CAS-owned because it resolves typed immutable object identity to
//! the transaction-private carrier locator used for one complete admission.
//! It is deliberately crate-private and fails closed when its frozen probe
//! budget is exhausted.

use super::{FsCasControlV1, FsCasErrorV1, FsOperationSpoolV1};
use crate::format::{validate_physical_object_len, PhysicalObjectKindV1};
use crate::object::TypedPhysicalObjectIdV1;
use crate::{CoreError, CoreResult};

pub(crate) const GLOBAL_SEEN_RECORD_BYTES: u64 = 64;
// One lookup is bounded independently of root/object count. The table remains
// file-backed; an adversarial cluster that cannot be resolved within this
// frozen budget fails closed instead of degrading into quadratic work.
pub(crate) const GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1: u32 = 256;
const GLOBAL_SEEN_CONTROL_POLL_PROBES_V1: u32 = 64;

#[derive(Clone, Copy)]
pub(crate) struct GlobalSeenRecordV1 {
    pub(crate) complete_len: u64,
    pub(crate) private_payload_offset: u64,
    pub(crate) carrier_ordinal: u32,
}

pub(crate) struct GlobalSeenLookupV1 {
    pub(crate) record: Option<GlobalSeenRecordV1>,
    pub(crate) vacant_slot: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GlobalSeenErrorV1 {
    Core(CoreError),
    FsCas(FsCasErrorV1),
}

impl From<CoreError> for GlobalSeenErrorV1 {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<FsCasErrorV1> for GlobalSeenErrorV1 {
    fn from(error: FsCasErrorV1) -> Self {
        Self::FsCas(error)
    }
}

pub(crate) struct FileGlobalSeenSpoolV1 {
    storage: FsOperationSpoolV1,
    capacity: u32,
    count: u32,
    lookups: u64,
    probes: u64,
    pub(crate) maximum_probe: u32,
}

impl FileGlobalSeenSpoolV1 {
    pub(crate) fn new(storage: FsOperationSpoolV1) -> Self {
        Self {
            storage,
            capacity: 0,
            count: 0,
            lookups: 0,
            probes: 0,
            maximum_probe: 0,
        }
    }

    pub(crate) fn initialize(&mut self, capacity: u32) -> Result<(), GlobalSeenErrorV1> {
        let mut control = super::fs::ContinueFsCasControlV1;
        self.initialize_controlled_v1(capacity, &mut control)
    }

    pub(crate) fn initialize_controlled_v1<C>(
        &mut self,
        capacity: u32,
        control: &mut C,
    ) -> Result<(), GlobalSeenErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if capacity < 8 || !capacity.is_power_of_two() {
            return Err(CoreError::ResourceRefused.into());
        }
        let len = u64::from(capacity)
            .checked_mul(GLOBAL_SEEN_RECORD_BYTES)
            .ok_or(CoreError::IntegerOverflow)?;
        self.storage
            .initialize_zeroed_len_controlled_v1(len, control)?;
        self.capacity = capacity;
        self.count = 0;
        Ok(())
    }

    /// Clear only the operation-wide lookup contents while retaining the
    /// already-granted file and its exact capacity. Cumulative lookup/probe
    /// observations intentionally survive the transition from construction
    /// de-duplication to candidate-root closure discovery.
    pub(crate) fn reset_for_candidate_graph_v1(&mut self) -> Result<(), GlobalSeenErrorV1> {
        let mut control = super::fs::ContinueFsCasControlV1;
        self.reset_for_candidate_graph_controlled_v1(&mut control)
    }

    pub(crate) fn reset_for_candidate_graph_controlled_v1<C>(
        &mut self,
        control: &mut C,
    ) -> Result<(), GlobalSeenErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if self.capacity == 0 {
            return Err(CoreError::ResourceRefused.into());
        }
        let len = u64::from(self.capacity)
            .checked_mul(GLOBAL_SEEN_RECORD_BYTES)
            .ok_or(CoreError::IntegerOverflow)?;
        self.storage.set_len_controlled_v1(0, control)?;
        self.storage
            .initialize_zeroed_len_controlled_v1(len, control)?;
        self.count = 0;
        Ok(())
    }

    pub(crate) fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        self.storage.resident_memory_bound_bytes()
    }

    pub(crate) fn cleanup_controlled_v1<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.storage.cleanup_controlled_v1(control)
    }

    pub(crate) fn retained_cleanup_terminal_v1(&self) -> Option<FsCasErrorV1> {
        self.storage.retained_cleanup_terminal_v1()
    }

    pub(crate) fn storage_bytes(&self) -> u64 {
        u64::from(self.capacity) * GLOBAL_SEEN_RECORD_BYTES
    }

    pub(crate) fn lookup<C>(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        control: &mut C,
    ) -> Result<GlobalSeenLookupV1, GlobalSeenErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if self.capacity == 0 {
            return Err(CoreError::ResourceRefused.into());
        }
        self.lookups = self
            .lookups
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let mask = u64::from(self.capacity - 1);
        let first = global_seen_hash_v1(id) & mask;
        let probe_budget = self.capacity.min(GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1);
        for relative in 0..probe_budget {
            if relative % GLOBAL_SEEN_CONTROL_POLL_PROBES_V1 == 0 {
                if control.cancellation_requested() {
                    return Err(CoreError::Cancelled.into());
                }
                if control.deadline_exceeded() {
                    return Err(CoreError::Deadline.into());
                }
            }
            let slot = u32::try_from((first + u64::from(relative)) & mask)
                .map_err(|_| CoreError::IntegerOverflow)?;
            let mut bytes = [0_u8; GLOBAL_SEEN_RECORD_BYTES as usize];
            self.storage.read_exact_at(
                u64::from(slot)
                    .checked_mul(GLOBAL_SEEN_RECORD_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
                &mut bytes,
            )?;
            let probe = relative.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
            self.probes = self
                .probes
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            self.maximum_probe = self.maximum_probe.max(probe);
            match bytes[0] {
                0 => {
                    if bytes[1..].iter().any(|byte| *byte != 0) {
                        return Err(CoreError::Reserved.into());
                    }
                    return Ok(GlobalSeenLookupV1 {
                        record: None,
                        vacant_slot: slot,
                    });
                }
                1 => {
                    if global_seen_key_matches_v1(&bytes, id)? {
                        return Ok(GlobalSeenLookupV1 {
                            record: Some(decode_global_seen_record_v1(&bytes)?),
                            vacant_slot: slot,
                        });
                    }
                }
                _ => return Err(CoreError::Reserved.into()),
            }
        }
        Err(CoreError::CountCap.into())
    }

    pub(crate) fn insert(
        &mut self,
        slot: u32,
        id: TypedPhysicalObjectIdV1,
        record: GlobalSeenRecordV1,
    ) -> Result<(), GlobalSeenErrorV1> {
        let mut control = super::fs::ContinueFsCasControlV1;
        self.insert_controlled_v1(slot, id, record, &mut control)
    }

    pub(crate) fn insert_controlled_v1<C>(
        &mut self,
        slot: u32,
        id: TypedPhysicalObjectIdV1,
        record: GlobalSeenRecordV1,
        control: &mut C,
    ) -> Result<(), GlobalSeenErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if slot >= self.capacity || self.count >= self.capacity / 2 {
            return Err(CoreError::CountCap.into());
        }
        self.storage.write_exact_at_controlled_v1(
            u64::from(slot)
                .checked_mul(GLOBAL_SEEN_RECORD_BYTES)
                .ok_or(CoreError::IntegerOverflow)?,
            &encode_global_seen_record_v1(id, record),
            control,
        )?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) const fn work_observation(&self) -> (u64, u64, u32, u32) {
        (self.lookups, self.probes, self.maximum_probe, self.count)
    }

    pub(crate) const fn direct_storage_observation(&self) -> (u64, u64, u64) {
        self.storage.direct_storage_observation()
    }
}

pub(crate) fn global_seen_hash_v1(id: TypedPhysicalObjectIdV1) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64
        ^ u64::try_from(kind_index(id.kind()) + 1).expect("five object kinds");
    for byte in id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ (hash >> 32)
}

const fn kind_index(kind: PhysicalObjectKindV1) -> usize {
    match kind {
        PhysicalObjectKindV1::VersionRecord => 0,
        PhysicalObjectKindV1::Tree => 1,
        PhysicalObjectKindV1::File => 2,
        PhysicalObjectKindV1::Symlink => 3,
        PhysicalObjectKindV1::Chunk => 4,
    }
}

fn global_seen_key_matches_v1(
    bytes: &[u8; GLOBAL_SEEN_RECORD_BYTES as usize],
    id: TypedPhysicalObjectIdV1,
) -> CoreResult<bool> {
    if bytes[2..8].iter().any(|byte| *byte != 0) || bytes[60..64].iter().any(|byte| *byte != 0) {
        return Err(CoreError::Reserved);
    }
    let kind = u8::try_from(kind_index(id.kind()) + 1).map_err(|_| CoreError::TypeDomain)?;
    Ok(bytes[1] == kind && bytes[8..40] == id.as_bytes()[..])
}

fn encode_global_seen_record_v1(
    id: TypedPhysicalObjectIdV1,
    record: GlobalSeenRecordV1,
) -> [u8; GLOBAL_SEEN_RECORD_BYTES as usize] {
    let mut bytes = [0_u8; GLOBAL_SEEN_RECORD_BYTES as usize];
    bytes[0] = 1;
    bytes[1] = u8::try_from(kind_index(id.kind()) + 1).expect("five object kinds");
    bytes[8..40].copy_from_slice(id.as_bytes());
    bytes[40..48].copy_from_slice(&record.complete_len.to_be_bytes());
    bytes[48..56].copy_from_slice(&record.private_payload_offset.to_be_bytes());
    bytes[56..60].copy_from_slice(&record.carrier_ordinal.to_be_bytes());
    bytes
}

fn decode_global_seen_record_v1(
    bytes: &[u8; GLOBAL_SEEN_RECORD_BYTES as usize],
) -> CoreResult<GlobalSeenRecordV1> {
    let complete_len = u64::from_be_bytes(bytes[40..48].try_into().map_err(|_| CoreError::Schema)?);
    validate_physical_object_len(complete_len)?;
    Ok(GlobalSeenRecordV1 {
        complete_len,
        private_payload_offset: u64::from_be_bytes(
            bytes[48..56].try_into().map_err(|_| CoreError::Schema)?,
        ),
        carrier_ordinal: u32::from_be_bytes(
            bytes[56..60].try_into().map_err(|_| CoreError::Schema)?,
        ),
    })
}

#[cfg(all(test, feature = "operation-polymorphism"))]
mod tests {
    use super::{
        global_seen_hash_v1, FileGlobalSeenSpoolV1, GlobalSeenErrorV1,
        GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1,
    };
    use crate::cas::fs::ContinueFsCasControlV1;
    use crate::cas::FsCasV1;
    use crate::identity::PhysicalChunkIdV1;
    use crate::object::TypedPhysicalObjectIdV1;
    use crate::CoreError;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn chunk_id(seed: u64) -> TypedPhysicalObjectIdV1 {
        let mut digest = [0_u8; 32];
        digest[..8].copy_from_slice(&seed.to_be_bytes());
        TypedPhysicalObjectIdV1::Chunk(PhysicalChunkIdV1::from_digest(digest))
    }

    #[test]
    fn file_backed_index_reaches_the_real_maximum_collision_probe() {
        const CAPACITY: u32 = 512;
        const INSERTIONS: usize = 256;
        let target = chunk_id(u64::MAX);
        let mask = u64::from(CAPACITY - 1);
        let target_bucket = global_seen_hash_v1(target) & mask;
        let mut colliding = Vec::with_capacity(INSERTIONS);
        let mut seed = 0_u64;
        while colliding.len() < INSERTIONS {
            let candidate = chunk_id(seed);
            seed = seed.checked_add(1).unwrap();
            if candidate != target
                && global_seen_hash_v1(candidate) & mask == target_bucket
                && !colliding.contains(&candidate)
            {
                colliding.push(candidate);
            }
        }

        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!(
            "layerfs-locator-index-collision-{}-{timestamp}",
            std::process::id()
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let mut control = ContinueFsCasControlV1;
        let storage = cas
            .begin_operation_spool_v1("locator-index-collision", &mut control)
            .unwrap();
        let mut index = FileGlobalSeenSpoolV1::new(storage);
        index
            .initialize_controlled_v1(CAPACITY, &mut control)
            .unwrap();

        for (ordinal, id) in colliding.iter().copied().enumerate() {
            let lookup = index.lookup(id, &mut control).unwrap();
            assert!(lookup.record.is_none());
            index
                .insert_controlled_v1(
                    lookup.vacant_slot,
                    id,
                    super::GlobalSeenRecordV1 {
                        complete_len: 1,
                        private_payload_offset: ordinal as u64,
                        carrier_ordinal: ordinal as u32,
                    },
                    &mut control,
                )
                .unwrap();
        }

        assert!(matches!(
            index.lookup(target, &mut control),
            Err(GlobalSeenErrorV1::Core(CoreError::CountCap))
        ));
        let (lookups, probes, maximum_probe, count) = index.work_observation();
        assert_eq!(lookups, u64::from(INSERTIONS as u32) + 1);
        let expected_probes = (INSERTIONS as u64 * (INSERTIONS as u64 + 1)) / 2
            + u64::from(GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1);
        assert_eq!(probes, expected_probes);
        assert_eq!(maximum_probe, GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1);
        assert_eq!(count, INSERTIONS as u32);

        index.cleanup_controlled_v1(&mut control).unwrap();
        drop(index);
        drop(cas);
        fs::remove_dir_all(root).unwrap();
    }
}
