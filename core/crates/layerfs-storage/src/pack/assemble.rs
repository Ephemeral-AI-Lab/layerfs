//! Group framing and the one selected pack assembly per write.
//!
//! Groups are framed exactly as their lane's grammar requires, and a pack is
//! assembled from the groups it will contain in one pass. Placement decides what
//! goes into the write before any bytes are assembled, so no candidate pack is
//! built and then discarded.

use crate::encoding::codec::{CompressionWorkspace, GROUP_LIMIT};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::{
    assembled_length, EncodedGroup, GroupCodec, PackLane, DIRECTORY_ENTRY_LEN, HEADER_LEN,
    PACK_MAGIC, WHOLE_FILE_COMPACT_DROP,
};
use crate::policy::{GROUP_COUNT_LIMIT, PACK_LIMIT, RECORD_COUNT_LIMIT};

/// Read tag of a record stored without a delta base.
pub const FULL_TAG: u8 = 0;

/// Frames `records` into one group body for the ordinary or native lane.
pub fn frame_group(records: &[Vec<u8>]) -> StorageResult<Vec<u8>> {
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
    if length > GROUP_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "pack.group_body",
            limit: GROUP_LIMIT as u64,
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

/// Framed length `records` would occupy as one group body.
pub fn framed_length(records: &[Vec<u8>]) -> StorageResult<usize> {
    let framing = 4_usize
        .checked_add(
            4_usize
                .checked_mul(records.len())
                .ok_or(StorageError::Integrity("group framing"))?,
        )
        .ok_or(StorageError::Integrity("group framing"))?;
    records.iter().try_fold(framing, |total, record| {
        total
            .checked_add(record.len())
            .ok_or(StorageError::Integrity("group length"))
    })
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
pub fn assemble(lane: PackLane, groups: &[EncodedGroup]) -> StorageResult<Vec<u8>> {
    if groups.is_empty() || groups.len() > GROUP_COUNT_LIMIT {
        return Err(StorageError::Integrity("pack group count"));
    }
    let length = assembled_length(lane, groups)?;
    if length > PACK_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "pack.assembled_length",
            limit: PACK_LIMIT as u64,
            actual: length as u64,
        });
    }
    let mut bytes = Vec::with_capacity(length);
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
            for group in groups {
                // The compact lane stores the record tag and the frame; the two
                // little-endian length fields are dropped and re-derived from the
                // canonical length recorded with the locator.
                bytes.push(group.bytes[0]);
                bytes.extend_from_slice(&group.bytes[1 + WHOLE_FILE_COMPACT_DROP..]);
            }
        }
        PackLane::Ordinary | PackLane::Native => {
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
            for group in groups {
                bytes.extend_from_slice(&group.bytes);
            }
        }
    }
    if bytes.len() != length {
        return Err(StorageError::Integrity("assembled pack length"));
    }
    Ok(bytes)
}
