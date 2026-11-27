//! Complete encoded groups from immutable physical input; consumers authenticate records.
use crate::{
    error::{StorageError, StorageResult},
    location::{PackDomain, PackInfo},
    pack::layout::{self, GroupView, PackHeader},
    port::{AcquiredPackUnits, PackRangeBodies, PersistedPackRanges},
};

/// One immutable complete encoded group; construction validates the full directory
/// and exact selected extent. Canonical consumers authenticate requested records;
/// unrelated records in an acquired group are not thereby authenticated.
#[derive(Debug)]
pub struct PackUnit {
    pub(crate) info: PackInfo,
    pub(crate) number: usize,
    pub(crate) header: PackHeader,
    pub(crate) view: GroupView,
    pub(crate) body: Vec<u8>,
}
impl PackUnit {
    /// Converts verified ranges into complete encoded groups. Ranges that split
    /// groups, duplicate them or bind a mismatched domain are refused.
    pub fn from_ranges(read: PersistedPackRanges) -> StorageResult<Vec<Self>> {
        let (info, prefix, pieces) = read.into_parts();
        Self::from_parts(info, &prefix, pieces)
    }
    /// Converts untrusted physical units into structurally checked encoded groups.
    /// Canonical consumers still authenticate each requested object/dependency.
    pub fn from_acquired(read: AcquiredPackUnits) -> StorageResult<Vec<Self>> {
        let (info, prefix, pieces) = read.into_parts();
        Self::from_parts(info, &prefix, pieces)
    }
    fn from_parts(
        info: PackInfo,
        prefix: &[u8],
        pieces: PackRangeBodies,
    ) -> StorageResult<Vec<Self>> {
        let header = layout::parse_directory_header(prefix, info.length)?;
        if PackDomain::for_lane(header.lane) != info.domain {
            return Err(StorageError::Integrity("sealed pack length/domain"));
        }
        let views: Vec<_> = layout::directory_group_views(prefix, header)?
            .into_iter()
            .enumerate()
            .collect();
        if header.lane == layout::PackLane::Singleton {
            return Err(StorageError::Integrity(
                "singleton requires whole pack acquisition",
            ));
        }
        let mut units = Vec::new();
        let mut prior = None;
        for (range, body) in pieces {
            if body.len() != range.length {
                return Err(StorageError::Integrity("selected group length"));
            }
            let end = range
                .offset
                .checked_add(range.length)
                .ok_or(StorageError::Integrity("selected group end"))?;
            let chosen: Vec<_> = views
                .iter()
                .filter(|(_, view)| view.start >= range.offset && view.end <= end)
                .copied()
                .collect();
            if chosen.first().map(|(_, view)| view.start) != Some(range.offset)
                || chosen.last().map(|(_, view)| view.end) != Some(end)
            {
                return Err(StorageError::Integrity("selected complete group extent"));
            }
            if chosen.len() == 1 {
                let (number, view) = chosen[0];
                if prior.is_some_and(|old| old >= number) {
                    return Err(StorageError::Integrity("selected group order"));
                }
                prior = Some(number);
                units.push(Self {
                    info,
                    number,
                    header,
                    view,
                    body,
                });
            } else {
                for (number, view) in chosen {
                    if prior.is_some_and(|old| old >= number) {
                        return Err(StorageError::Integrity("selected group order"));
                    }
                    prior = Some(number);
                    let selected = body
                        .get(view.start - range.offset..view.end - range.offset)
                        .ok_or(StorageError::Integrity("coalesced group extent"))?
                        .to_vec();
                    units.push(Self {
                        info,
                        number,
                        header,
                        view,
                        body: selected,
                    });
                }
            }
        }
        Ok(units)
    }
}
/// Physical input chosen by a Source before decoding; no error-driven alternative.
#[derive(Debug)]
pub enum PackAcquisition {
    /// Complete immutable body; private sources may lack a persisted descriptor.
    Whole {
        /// Real persisted descriptor, when this is acknowledged input.
        info: Option<PackInfo>,
        /// Complete real bytes.
        body: Vec<u8>,
    },
    /// Complete structurally checked groups; consumers authenticate used records.
    Units(Vec<PackUnit>),
}
/// A borrow of a complete validated group from either cache representation.
#[derive(Clone, Copy, Debug)]
pub struct GroupSlice<'a> {
    /// Validated control/directory grammar for the original complete pack.
    pub header: PackHeader,
    /// Original physical extent and codec/decoded-length bounds.
    pub view: GroupView,
    /// Complete encoded group body.
    pub bytes: &'a [u8],
}
