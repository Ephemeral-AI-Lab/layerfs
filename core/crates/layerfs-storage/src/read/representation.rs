//! Locator facts and payload-record widths for bounded storage observation.
//!
//! An observation reads what the Store already recorded: each object's role
//! and canonical length from its locator, and, for a WholeFile or Chunk object,
//! the width of its own stored record and the direct base that record names.
//! Group directories, pack headers and the records of any dependency are not
//! assigned to the object; nothing here reconstructs a payload.
use super::Fetch;
use crate::{
    encoding::{delta::record::stored_base, framed_record, PackCache},
    error::{StorageError, StorageResult},
    location::ObjectLocation,
    pack::layout::{GroupCodec, PackLane},
    policy::StorageCapacities,
    source::Source,
};
use layerfs_content::{ObjectId, ObjectRole};

/// One payload object's own stored record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadRecord {
    /// Exact bytes of the record inside its group, without shared framing.
    pub encoded_bytes: u64,
    /// The record's direct delta base, when it is stored against one.
    pub base: Option<ObjectId>,
}

/// What the Store recorded for one object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescribedObject {
    /// Logical role from the locator.
    pub role: ObjectRole,
    /// Canonical length recorded at save time.
    pub canonical_length: u64,
    /// For a WholeFile or Chunk object when records were asked for: its own
    /// record, or `None` when the record sits in a compressed group and has
    /// no width of its own. Always `None` for other roles.
    pub payload_record: Option<PayloadRecord>,
}

/// Whether `role` is a payload object: file content rather than metadata.
pub const fn is_payload(role: ObjectRole) -> bool {
    matches!(role, ObjectRole::WholeFile | ObjectRole::Chunk)
}

pub(super) fn describe(
    packs: &mut PackCache,
    source: &Fetch,
    capacities: &StorageCapacities,
    ids: &[ObjectId],
    records: bool,
) -> StorageResult<Vec<DescribedObject>> {
    if ids.len() > capacities.read_objects {
        return Err(StorageError::CapacityExceeded {
            what: "storage.describe_objects",
            limit: capacities.read_objects as u64,
            actual: ids.len() as u64,
        });
    }
    source.begin_demand();
    source.locate(ids)?;
    ids.iter()
        .map(|id| {
            let location = source
                .location(*id, i64::MAX)?
                .ok_or(StorageError::ObjectMissing(*id))?;
            let payload_record = if records && is_payload(location.role) {
                payload_record(packs, source, &location)?
            } else {
                None
            };
            Ok(DescribedObject {
                role: location.role,
                canonical_length: location.canonical_length as u64,
                payload_record,
            })
        })
        .collect()
}

fn payload_record(
    packs: &mut PackCache,
    source: &Fetch,
    location: &ObjectLocation,
) -> StorageResult<Option<PayloadRecord>> {
    packs.acquire_groups(source, location.pack_id, &[location.group_number])?;
    let group = packs
        .group(source, location.pack_id, location.group_number)?
        .ok_or(StorageError::Integrity("encoded group cache"))?;
    let lane = group.header.lane;
    if group.view.codec != GroupCodec::Raw || lane == PackLane::Ordinary {
        return Ok(None);
    }
    let ordinal = if lane == PackLane::Singleton {
        0
    } else {
        location.record_number
    };
    let record = framed_record(group.bytes, ordinal)?;
    Ok(Some(PayloadRecord {
        encoded_bytes: record.len() as u64,
        base: stored_base(lane, record, location.canonical_length, location.role)?,
    }))
}
