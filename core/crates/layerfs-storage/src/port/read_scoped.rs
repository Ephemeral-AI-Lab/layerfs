//! Physical acquisition for canonical consumers; unread pack bytes are not audited.
use super::{
    PackRangeBodies, PackReadChoice, PackReadPlan, PersistedPack, PersistenceError,
    PACK_READ_PREFIX_BYTES,
};
use crate::location::PackInfo;
use crate::{location::PackDomain, pack::layout, policy};

/// Bounded physical units. These bytes are structurally checked by storage,
/// then authenticated by canonical reconstruction or a value-catalogue digest.
/// This carrier makes no whole-pack or unread-record integrity claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcquiredPackUnits {
    info: PackInfo,
    prefix: Vec<u8>,
    ranges: PackRangeBodies,
    acquired_bytes: usize,
}
impl AcquiredPackUnits {
    /// Bound descriptor; its digest is a whole-pack audit identity only.
    pub const fn info(&self) -> PackInfo {
        self.info
    }
    /// Initially untrusted bounded control bytes.
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }
    /// Exact complete encoded units selected before body reads.
    pub fn ranges(&self) -> &PackRangeBodies {
        &self.ranges
    }
    /// Actual requested bytes, including controls and without overlap duplication.
    pub const fn acquired_bytes(&self) -> usize {
        self.acquired_bytes
    }
    /// Transfers physical bytes without asserting whole-pack authentication.
    pub fn into_parts(self) -> (PackInfo, Vec<u8>, PackRangeBodies) {
        (self.info, self.prefix, self.ranges)
    }
}
/// One planned acquisition for canonical consumers; complete choices keep SHA256.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcquiredPackRead {
    /// Complete immutable length/SHA256-authenticated pack.
    Whole(PersistedPack),
    /// Selected physical units requiring consumer authentication.
    Units(AcquiredPackUnits),
}
impl AcquiredPackRead {
    /// Acquires only the prefix and selected complete units from real offset I/O.
    /// No retry, whole-body fallback, expected-byte prefill or partial pack hash.
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
        let mut prefix = vec![0; layout::HEADER_LEN];
        read_at(0, &mut prefix)?;
        let header = layout::parse_control_header(&prefix, info.length)
            .map_err(|_| PersistenceError::Malformed)?;
        if header.body_offset > PACK_READ_PREFIX_BYTES
            || PackDomain::for_lane(header.lane) != info.domain
        {
            return Err(PersistenceError::Malformed);
        }
        prefix.resize(header.body_offset, 0);
        read_at(layout::HEADER_LEN, &mut prefix[layout::HEADER_LEN..])?;
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
                super::read_selection::check_ranges(info, prefix.len(), &ranges)?;
                let mut acquired_bytes = prefix.len();
                let mut pieces = Vec::with_capacity(ranges.len());
                for range in ranges {
                    let mut body = vec![0; range.length];
                    let copied = prefix.len().saturating_sub(range.offset).min(range.length);
                    if copied > 0 {
                        body[..copied]
                            .copy_from_slice(&prefix[range.offset..range.offset + copied]);
                    }
                    if copied < range.length {
                        read_at(range.offset + copied, &mut body[copied..])?;
                        acquired_bytes += range.length - copied;
                    }
                    pieces.push((range, body));
                }
                Ok(Self::Units(AcquiredPackUnits {
                    info,
                    prefix,
                    ranges: pieces,
                    acquired_bytes,
                }))
            }
        }
    }
}
