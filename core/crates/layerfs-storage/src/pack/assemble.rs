//! Group framing and the one selected pack assembly per write.
//!
//! Groups are framed exactly as their lane's grammar requires, and a pack is
//! assembled from the groups it will contain in one pass. Placement decides what
//! goes into the write before any bytes are assembled, so no candidate pack is
//! built and then discarded. The singleton lane shares the ordinary directory
//! grammar but holds exactly one raw group and is bounded by its own pack limit.

use crate::encoding::codec::{CompressionWorkspace, GROUP_LIMIT};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::{
    assembled_length, EncodedGroup, GroupCodec, PackLane, DIRECTORY_ENTRY_LEN, HEADER_LEN,
    PACK_MAGIC, WHOLE_FILE_COMPACT_DROP,
};
use crate::policy::RECORD_COUNT_LIMIT;

/// Read tag of a record stored without a delta base.
pub const FULL_TAG: u8 = 0;

/// Frames `records` into one group body for the ordinary or native lane.
pub fn frame_group(records: &[Vec<u8>]) -> StorageResult<Vec<u8>> {
    frame_group_bounded(records, GROUP_LIMIT)
}

/// Frames `records` into one group body bounded by `limit`.
///
/// The singleton lane needs the same grammar with its own, larger body bound; the
/// ordinary and native lanes keep the group ceiling.
pub fn frame_group_bounded(records: &[Vec<u8>], limit: usize) -> StorageResult<Vec<u8>> {
    if records.is_empty() || records.len() > RECORD_COUNT_LIMIT {
        return Err(StorageError::Integrity("group record count"));
    }
    let framing = 4_usize
        .checked_add(
            4_usize
                .checked_mul(records.len())
                .ok_or(StorageError::Integrity("group framing"))?,
        )
        .ok_or(StorageError::Integrity("group framing"))?;
    let mut length = framing;
    for record in records {
        if record.is_empty() {
            return Err(StorageError::Integrity("empty record"));
        }
        length = length
            .checked_add(record.len())
            .ok_or(StorageError::Integrity("group length"))?;
    }
    if length > limit {
        return Err(StorageError::CapacityExceeded {
            what: "pack.group_body",
            limit: limit as u64,
            actual: length as u64,
        });
    }
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(
        &u32::try_from(records.len())
            .map_err(|_| StorageError::Integrity("group record count"))?
            .to_le_bytes(),
    );
    let mut end = 0_usize;
    for record in records {
        end += record.len();
        bytes.extend_from_slice(
            &u32::try_from(end)
                .map_err(|_| StorageError::Integrity("group record end"))?
                .to_le_bytes(),
        );
    }
    for record in records {
        bytes.extend_from_slice(record);
    }
    Ok(bytes)
}

/// Framed length of one group body holding `records` records of `payload` bytes.
///
/// A group body is one 4-byte record count, one 4-byte end offset per record and
/// the records themselves. This is the shared framing identity: a caller that
/// accumulates a group record by record must project through it, because adding a
/// per-record framed length instead counts the shared framing once per record and
/// over-counts it by `4n - 4`.
pub fn framed_group_length(records: usize, payload: usize) -> StorageResult<usize> {
    let framing = 4_usize
        .checked_add(
            4_usize
                .checked_mul(records)
                .ok_or(StorageError::Integrity("group framing"))?,
        )
        .ok_or(StorageError::Integrity("group framing"))?;
    framing
        .checked_add(payload)
        .ok_or(StorageError::Integrity("group length"))
}

/// Framed length `records` would occupy as one group body.
pub fn framed_length(records: &[Vec<u8>]) -> StorageResult<usize> {
    let payload = records.iter().try_fold(0_usize, |total, record| {
        total
            .checked_add(record.len())
            .ok_or(StorageError::Integrity("group length"))
    })?;
    framed_group_length(records.len(), payload)
}

/// Builds the placement-ready group for `lane` from already-framed records.
///
/// The ordinary lane may retain a compressed body; the native and whole-file
/// lanes store raw bodies, matching their grammars.
pub fn build_group(
    lane: PackLane,
    records: &[Vec<u8>],
    workspace: Option<&mut CompressionWorkspace>,
) -> StorageResult<EncodedGroup> {
    match lane {
        PackLane::WholeFile => {
            if records.len() != 1 {
                return Err(StorageError::Integrity("compact group record count"));
            }
            let bytes = records[0].clone();
            if bytes.len() <= WHOLE_FILE_COMPACT_DROP {
                return Err(StorageError::Integrity("compact record width"));
            }
            Ok(EncodedGroup {
                decoded_length: bytes.len(),
                bytes,
                records: 1,
                codec: GroupCodec::Raw,
            })
        }
        PackLane::Native => Ok(EncodedGroup {
            decoded_length: framed_length(records)?,
            bytes: frame_group(records)?,
            records: records.len(),
            codec: GroupCodec::Raw,
        }),
        PackLane::Singleton => Ok(EncodedGroup {
            decoded_length: framed_length(records)?,
            bytes: frame_group_bounded(records, crate::policy::SINGLETON_PACK_LIMIT)?,
            records: records.len(),
            codec: GroupCodec::Raw,
        }),
        PackLane::PooledMetadata => {
            if records.len() != 1 {
                return Err(StorageError::Integrity("pooled group record count"));
            }
            let bytes = frame_group(records)?;
            if bytes.len() > crate::policy::METADATA_GROUP_LIMIT {
                return Err(StorageError::CapacityExceeded {
                    what: "pack.pooled_group_body",
                    limit: crate::policy::METADATA_GROUP_LIMIT as u64,
                    actual: bytes.len() as u64,
                });
            }
            Ok(EncodedGroup {
                decoded_length: bytes.len(),
                bytes,
                records: 1,
                codec: GroupCodec::Raw,
            })
        }
        PackLane::Ordinary => {
            let raw = frame_group(records)?;
            let decoded_length = raw.len();
            let mut bytes = raw;
            let mut codec = GroupCodec::Raw;
            if decoded_length <= GROUP_LIMIT {
                if let Some(workspace) = workspace {
                    if let Some(frame) =
                        crate::encoding::codec::compress_group_body(workspace, &bytes)?
                    {
                        bytes = frame;
                        codec = GroupCodec::Zstandard;
                    }
                }
            }
            Ok(EncodedGroup {
                decoded_length,
                bytes,
                records: records.len(),
                codec,
            })
        }
    }
}

/// Assembles the selected groups into the exact pack bytes to write.
///
/// The groups stay borrowed: the caller retains them as the lane's open tail, so
/// the assembled pack and the retained tail are live together by design. Use
/// [`assemble_consuming`] when the caller is closing the pack instead, which
/// releases each constituent body as it is copied.
pub fn assemble(lane: PackLane, groups: &[EncodedGroup]) -> StorageResult<Vec<u8>> {
    let length = checked_pack_length(lane, groups)?;
    let mut bytes = Vec::with_capacity(length);
    write_header_and_directory(lane, groups, &mut bytes)?;
    for group in groups {
        append_body(lane, group, &mut bytes)?;
    }
    if bytes.len() != length {
        return Err(StorageError::Integrity("assembled pack length"));
    }
    Ok(bytes)
}

/// Assembles the groups of a pack that is being closed, releasing each one.
///
/// The lane's retained open tail exists so that a later group can append to the
/// pack without moving any group or record ordinal. A pack that is being *closed*
/// needs no tail, so the caller hands its groups over and every constituent body
/// is dropped as soon as it has been copied into the assembly. Peak live bytes
/// are then the assembly plus the groups not yet copied - approximately one pack
/// - instead of the assembly plus the whole retained tail.
pub fn assemble_consuming(lane: PackLane, groups: Vec<EncodedGroup>) -> StorageResult<Vec<u8>> {
    let length = checked_pack_length(lane, &groups)?;
    let mut bytes = Vec::with_capacity(length);
    write_header_and_directory(lane, &groups, &mut bytes)?;
    // Bodies are appended in group order and each group is dropped at the end of
    // its own iteration, so the released bytes are the ones already copied.
    for group in groups {
        append_body(lane, &group, &mut bytes)?;
    }
    if bytes.len() != length {
        return Err(StorageError::Integrity("assembled pack length"));
    }
    Ok(bytes)
}

/// Validates a group set and returns the exact assembled length.
fn checked_pack_length(lane: PackLane, groups: &[EncodedGroup]) -> StorageResult<usize> {
    if groups.is_empty() || groups.len() > lane.group_count_limit() {
        return Err(StorageError::Integrity("pack group count"));
    }
    let length = assembled_length(lane, groups)?;
    if length > lane.pack_limit() {
        return Err(StorageError::CapacityExceeded {
            what: "pack.assembled_length",
            limit: lane.pack_limit() as u64,
            actual: length as u64,
        });
    }
    Ok(length)
}

fn append_body(lane: PackLane, group: &EncodedGroup, bytes: &mut Vec<u8>) -> StorageResult<()> {
    match lane {
        PackLane::WholeFile => {
            // The compact lane stores the record tag and the frame; the two
            // little-endian length fields are dropped and re-derived from the
            // canonical length recorded with the locator.
            bytes.push(group.bytes[0]);
            bytes.extend_from_slice(&group.bytes[1 + WHOLE_FILE_COMPACT_DROP..]);
        }
        PackLane::Ordinary | PackLane::Native | PackLane::PooledMetadata | PackLane::Singleton => {
            bytes.extend_from_slice(&group.bytes);
        }
    }
    Ok(())
}

/// Writes the pack header and the group directory.
fn write_header_and_directory(
    lane: PackLane,
    groups: &[EncodedGroup],
    bytes: &mut Vec<u8>,
) -> StorageResult<()> {
    bytes.extend_from_slice(&PACK_MAGIC);
    bytes.extend_from_slice(&lane.version().to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(groups.len())
            .map_err(|_| StorageError::Integrity("pack group count"))?
            .to_le_bytes(),
    );
    match lane {
        PackLane::WholeFile => {
            let mut offset = HEADER_LEN + 4 * groups.len();
            for group in groups {
                bytes.extend_from_slice(
                    &u32::try_from(offset)
                        .map_err(|_| StorageError::Integrity("compact start"))?
                        .to_le_bytes(),
                );
                offset += group.bytes.len() - WHOLE_FILE_COMPACT_DROP;
            }
        }
        PackLane::Ordinary | PackLane::Native | PackLane::PooledMetadata | PackLane::Singleton => {
            let mut offset = HEADER_LEN + DIRECTORY_ENTRY_LEN * groups.len();
            for group in groups {
                bytes.extend_from_slice(
                    &u32::try_from(offset)
                        .map_err(|_| StorageError::Integrity("group start"))?
                        .to_le_bytes(),
                );
                bytes.extend_from_slice(
                    &u32::try_from(group.bytes.len())
                        .map_err(|_| StorageError::Integrity("group encoded length"))?
                        .to_le_bytes(),
                );
                bytes.extend_from_slice(
                    &u32::try_from(group.decoded_length)
                        .map_err(|_| StorageError::Integrity("group decoded length"))?
                        .to_le_bytes(),
                );
                bytes.extend_from_slice(&[
                    match group.codec {
                        GroupCodec::Raw => 0,
                        GroupCodec::Zstandard => 1,
                    },
                    0,
                    0,
                    0,
                ]);
                offset += group.bytes.len();
            }
        }
    }
    Ok(())
}
