//! FULL reconstruction: one stored record to its canonical object.
//!
//! Every intermediate is authenticated. The pack header and group directory are
//! validated before a body is touched, a compressed body is checked against its
//! declared decoded length, the record grammar is validated before use, and the
//! rebuilt canonical object must match the length recorded for its locator. A
//! corrupt record is an error and never selects another decoder.

use layerfs_content::encode_whole_file_payload;
use layerfs_content::file::mapping::encode_chunk_object;

use crate::encoding::codec::{CodecProfile, DecompressionWorkspace};
use crate::error::{StorageError, StorageResult};
use crate::pack::assemble::FULL_TAG;
use crate::pack::layout::{
    group_view, parse_header, record_range, GroupCodec, PackLane, WHOLE_FILE_COMPACT_DROP,
};
use crate::policy::{CANONICAL_LIMIT, OBJECT_ENVELOPE_OVERHEAD};

/// Rebuilds the canonical object stored at one locator.
pub fn decode_canonical(
    pack: &[u8],
    group_number: usize,
    record_number: usize,
    canonical_length: usize,
    workspace: &mut DecompressionWorkspace,
) -> StorageResult<Vec<u8>> {
    if canonical_length == 0 || canonical_length > CANONICAL_LIMIT {
        return Err(StorageError::Integrity("canonical length"));
    }
    let header = parse_header(pack)?;
    let view = group_view(pack, header, group_number)?;
    let selected = pack
        .get(view.start..view.end)
        .ok_or(StorageError::Integrity("group body range"))?;
    match header.lane {
        PackLane::Ordinary => {
            let body = match view.codec {
                GroupCodec::Raw => selected.to_vec(),
                GroupCodec::Zstandard => {
                    workspace.decompress_group(selected, view.decoded_length)?
                }
            };
            if body.len() != view.decoded_length {
                return Err(StorageError::Integrity("group body length"));
            }
            let record = framed_record(&body, record_number)?;
            if record.first() != Some(&FULL_TAG) {
                return Err(StorageError::Integrity("record tag is not FULL"));
            }
            let canonical = record[1..].to_vec();
            if canonical.len() != canonical_length {
                return Err(StorageError::Integrity("ordinary record length"));
            }
            Ok(canonical)
        }
        PackLane::Native => {
            if view.codec != GroupCodec::Raw {
                return Err(StorageError::Integrity("native group codec"));
            }
            let record = framed_record(selected, record_number)?;
            if record.first() != Some(&FULL_TAG) || record.len() < 5 {
                return Err(StorageError::Integrity("native record framing"));
            }
            let raw_length = u32::from_le_bytes(
                record[1..5]
                    .try_into()
                    .map_err(|_| StorageError::Integrity("native raw length"))?,
            ) as usize;
            let frame = &record[5..];
            let raw = workspace.decompress(CodecProfile::Native, frame, raw_length)?;
            let canonical = encode_chunk_object(&raw)?;
            if canonical.len() != canonical_length {
                return Err(StorageError::Integrity("native record length"));
            }
            Ok(canonical)
        }
        PackLane::WholeFile => {
            if record_number != 0 {
                return Err(StorageError::Integrity("compact record ordinal"));
            }
            if selected.len() <= WHOLE_FILE_COMPACT_DROP || selected[0] != FULL_TAG {
                return Err(StorageError::Integrity("compact record framing"));
            }
            let raw_length = canonical_length
                .checked_sub(OBJECT_ENVELOPE_OVERHEAD)
                .ok_or(StorageError::Integrity("compact canonical length"))?;
            let frame = &selected[1..];
            let raw = workspace.decompress(CodecProfile::Small, frame, raw_length)?;
            let canonical = encode_whole_file_payload(&raw)?;
            if canonical.len() != canonical_length {
                return Err(StorageError::Integrity("whole-file record length"));
            }
            Ok(canonical)
        }
    }
}

fn framed_record(group: &[u8], ordinal: usize) -> StorageResult<&[u8]> {
    if group.len() < 4 {
        return Err(StorageError::Integrity("group framing"));
    }
    let count = u32::from_le_bytes(
        group[..4]
            .try_into()
            .map_err(|_| StorageError::Integrity("group record count"))?,
    ) as usize;
    let ends = group
        .get(4..4 + 4 * count)
        .ok_or(StorageError::Integrity("group record directory"))?;
    let (start, end) = record_range(count, ends, group.len(), ordinal)?;
    group
        .get(start..end)
        .ok_or(StorageError::Integrity("record range"))
}
