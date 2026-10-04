//! Selected physical bytes authenticated by a complete, bounded pack scan.
use super::{ObjectKey, PersistedPack, PersistenceError};
use crate::{
    location::{PackDomain, PackInfo},
    pack::layout,
    policy,
};
use sha2::{Digest, Sha256};

/// Largest supported control/directory prefix supplied to a C2 planner.
pub const PACK_READ_PREFIX_BYTES: usize =
    layout::HEADER_LEN + layout::DIRECTORY_ENTRY_LEN * policy::GROUP_COUNT_LIMIT;
/// Authentication scratch shares the existing acquisition-byte allowance.
pub const PACK_SCAN_BYTES: usize = 64 * 1024;
/// One complete encoded-unit extent selected by storage, never a canonical slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackRange {
    /// Offset in the immutable physical BLOB.
    pub offset: usize,
    /// Number of encoded bytes, positive and within the descriptor.
    pub length: usize,
}
/// Owned range extents and their acquired bytes, in physical request order.
pub type PackRangeBodies = Vec<(PackRange, Vec<u8>)>;
/// A planned strategy, selected before body I/O; errors never change strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackReadChoice {
    /// Materialize the complete pack and use the existing authenticated carrier.
    Whole,
    /// Scan/authenticate the entire pack but materialize only these disjoint ranges.
    Ranges(Vec<PackRange>),
}
/// Storage-owned framing/unit planner over a bounded, initially untrusted prefix.
/// The provider supplies physical I/O only. Strict acquisition authenticates the
/// prefix with the full scan; scoped acquisition leaves physical bytes untrusted.
/// Callers validate returned framing and authenticate every used canonical object.
pub trait PackReadPlan {
    /// Choose complete encoded units under C2 grammar and demand bounds.
    fn select(&mut self, info: PackInfo, prefix: &[u8])
        -> Result<PackReadChoice, PersistenceError>;
}
/// Immutable selected ranges from a complete length/SHA256-authenticated scan.
/// This is not a PersistedPack: its returned bytes do not contain the entire pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedPackRanges {
    info: PackInfo,
    prefix: Vec<u8>,
    ranges: PackRangeBodies,
}
impl PersistedPackRanges {
    /// Complete pack descriptor checked by the real complete scan.
    pub const fn info(&self) -> PackInfo {
        self.info
    }
    /// Bounded prefix authenticated as part of that scan.
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }
    /// Exact selected ranges and immutable acquired bytes, in request order.
    pub fn ranges(&self) -> &[(PackRange, Vec<u8>)] {
        &self.ranges
    }
    /// Transfers authenticated physical pieces; mutated bytes require reacquisition.
    pub fn into_parts(self) -> (PackInfo, Vec<u8>, PackRangeBodies) {
        (self.info, self.prefix, self.ranges)
    }
}
/// One explicit whole/selected acquisition, with private authenticated carriers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PersistedPackRead {
    /// Existing complete-body length/SHA256 guarantee, unchanged.
    Whole(PersistedPack),
    /// Selected pieces whose authentication paid for a complete scan.
    Ranges(PersistedPackRanges),
}
impl PersistedPackRead {
    /// Acquires real bytes through exact offset reads. Every byte is read and
    /// hashed once; selected bytes are copied from those same observed chunks.
    /// No expected-result prefill or partial-byte whole-pack hash is accepted.
    pub fn acquire(
        info: PackInfo,
        plan: &mut dyn PackReadPlan,
        mut read_at: impl FnMut(usize, &mut [u8]) -> Result<(), PersistenceError>,
    ) -> Result<Self, PersistenceError> {
        if info.pack_id <= 0
            || !(32..=policy::SINGLETON_PACK_LIMIT).contains(&info.length)
            || info.domain == PackDomain::Metadata && info.length > policy::PACK_LIMIT
        {
            return Err(PersistenceError::Malformed);
        }
        let mut prefix = vec![0; info.length.min(PACK_READ_PREFIX_BYTES)];
        read_at(0, &mut prefix)?;
        match plan.select(info, &prefix)? {
            PackReadChoice::Whole => {
                let start = prefix.len();
                prefix.resize(info.length, 0);
                if start < info.length {
                    read_at(start, &mut prefix[start..])?;
                }
                Ok(Self::Whole(PersistedPack::authenticate(info, prefix)?))
            }
            PackReadChoice::Ranges(ranges) => {
                check_ranges(info, prefix.len(), &ranges)?;
                let mut pieces: Vec<_> = ranges
                    .into_iter()
                    .map(|range| (range, vec![0; range.length]))
                    .collect();
                let mut hash = Sha256::new();
                hash.update(&prefix);
                copy_selected(0, &prefix, &mut pieces);
                let mut offset = prefix.len();
                let mut scratch = vec![0; PACK_SCAN_BYTES.min(info.length - offset)];
                while offset < info.length {
                    let count = scratch.len().min(info.length - offset);
                    let chunk = &mut scratch[..count];
                    read_at(offset, chunk)?;
                    hash.update(&*chunk);
                    copy_selected(offset, chunk, &mut pieces);
                    offset += count;
                }
                if ObjectKey::from_bytes(hash.finalize().into()) != info.key {
                    return Err(PersistenceError::Malformed);
                }
                Ok(Self::Ranges(PersistedPackRanges {
                    info,
                    prefix,
                    ranges: pieces,
                }))
            }
        }
    }
}
pub(super) fn check_ranges(
    info: PackInfo,
    prefix: usize,
    ranges: &[PackRange],
) -> Result<(), PersistenceError> {
    if ranges.is_empty() || ranges.len() > policy::READ_OBJECT_LIMIT {
        return Err(PersistenceError::Malformed);
    }
    let mut end = 0;
    let mut bytes = prefix
        .checked_add(PACK_SCAN_BYTES.min(info.length - prefix))
        .ok_or(PersistenceError::Malformed)?;
    for range in ranges {
        if range.length == 0 || range.offset < end {
            return Err(PersistenceError::Malformed);
        }
        end = range
            .offset
            .checked_add(range.length)
            .ok_or(PersistenceError::Malformed)?;
        if end > info.length {
            return Err(PersistenceError::Malformed);
        }
        bytes = bytes
            .checked_add(range.length)
            .ok_or(PersistenceError::Malformed)?;
    }
    if bytes > policy::DEPENDENCY_PACK_CACHE_BYTES {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
fn copy_selected(offset: usize, chunk: &[u8], pieces: &mut [(PackRange, Vec<u8>)]) {
    let chunk_end = offset + chunk.len();
    for (range, body) in pieces {
        let start = offset.max(range.offset);
        let end = chunk_end.min(range.offset + range.length);
        if start < end {
            body[start - range.offset..end - range.offset]
                .copy_from_slice(&chunk[start - offset..end - offset]);
        }
    }
}
