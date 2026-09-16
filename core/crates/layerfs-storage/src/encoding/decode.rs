//! Record reconstruction: one stored record to its canonical object.
//!
//! Every intermediate is authenticated. The pack header and group directory are
//! validated before a body is touched, a compressed body is checked against its
//! declared decoded length, the record grammar is validated before use, and the
//! rebuilt canonical object must match the length recorded for its locator. A
//! corrupt record is an error and never selects another decoder. A PREFIX record
//! is decoded only against the exact base payload the resolver reconstructed and
//! authenticated; there is no trial decode and no fallback.

use layerfs_content::encode_whole_file_payload;
use layerfs_content::file::mapping::encode_chunk_object;
use layerfs_content::ObjectRole;

use crate::encoding::codec::{CodecProfile, DecompressionWorkspace};
use crate::encoding::delta::record;
use crate::error::{StorageError, StorageResult};
use crate::pack::assemble::FULL_TAG as ORDINARY_FULL_TAG;
use crate::pack::layout::{group_view, parse_header, record_range, GroupCodec, PackLane};
use crate::policy::{StorageCapacities, CANONICAL_LIMIT};
use crate::sqlite::lookup::ObjectLocation;

/// Rebuilds the canonical object stored at one locator.
///
/// `base` is the exact raw payload of the recorded direct base, already read and
/// authenticated by the caller, or `None` when the locator records no base. The
/// presence of the base must agree with the record's own tag.
pub fn decode_canonical(
    pack: &[u8],
    location: &ObjectLocation,
    capacities: &StorageCapacities,
    base: Option<&[u8]>,
    workspace: &mut DecompressionWorkspace,
) -> StorageResult<Vec<u8>> {
    let canonical_length = location.canonical_length;
    if canonical_length == 0 || canonical_length > CANONICAL_LIMIT {
        return Err(StorageError::Integrity("canonical length"));
    }
    let header = parse_header(pack)?;
    let view = group_view(pack, header, location.group_number)?;
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
            let bytes = framed_record(&body, location.record_number)?;
            if bytes.first() != Some(&ORDINARY_FULL_TAG) {
                return Err(StorageError::Integrity("record tag is not FULL"));
            }
            let canonical = bytes[1..].to_vec();
            if canonical.len() != canonical_length {
                return Err(StorageError::Integrity("ordinary record length"));
            }
            Ok(canonical)
        }
        PackLane::Native => {
            if view.codec != GroupCodec::Raw {
                return Err(StorageError::Integrity("native group codec"));
            }
            let bytes = framed_record(selected, location.record_number)?;
            let parsed = record::parse_native(bytes)?;
            let raw = decode_payload(
                PackLane::Native,
                parsed.raw_length,
                parsed.base.is_some(),
                parsed.frame,
                base,
                capacities,
                workspace,
            )?;
            let canonical = encode_chunk_object(&raw)?;
            if canonical.len() != canonical_length {
                return Err(StorageError::Integrity("native record length"));
            }
            Ok(canonical)
        }
        PackLane::WholeFile => {
            if location.record_number != 0 {
                return Err(StorageError::Integrity("compact record ordinal"));
            }
            let parsed = record::parse(PackLane::WholeFile, selected, canonical_length)?;
            let raw = decode_payload(
                PackLane::WholeFile,
                parsed.raw_length,
                parsed.base.is_some(),
                parsed.frame,
                base,
                capacities,
                workspace,
            )?;
            let canonical = encode_whole_file_payload(&raw)?;
            if canonical.len() != canonical_length {
                return Err(StorageError::Integrity("whole-file record length"));
            }
            Ok(canonical)
        }
        PackLane::Singleton => {
            if view.codec != GroupCodec::Raw || location.record_number != 0 {
                return Err(StorageError::Integrity("singleton record framing"));
            }
            let bytes = framed_record(selected, 0)?;
            let parsed = record::parse(PackLane::Singleton, bytes, canonical_length)?;
            let raw = decode_payload(
                PackLane::Singleton,
                parsed.raw_length,
                parsed.base.is_some(),
                parsed.frame,
                base,
                capacities,
                workspace,
            )?;
            let canonical = match location.role {
                ObjectRole::WholeFile => encode_whole_file_payload(&raw)?,
                ObjectRole::Chunk => encode_chunk_object(&raw)?,
                _ => return Err(StorageError::Integrity("singleton record role")),
            };
            if canonical.len() != canonical_length {
                return Err(StorageError::Integrity("singleton record length"));
            }
            Ok(canonical)
        }
        PackLane::PooledMetadata => Err(StorageError::Integrity(
            "pooled metadata record requires value groups",
        )),
    }
}

fn decode_payload(
    lane: PackLane,
    raw_length: usize,
    has_base: bool,
    frame: &[u8],
    base: Option<&[u8]>,
    capacities: &StorageCapacities,
    workspace: &mut DecompressionWorkspace,
) -> StorageResult<Vec<u8>> {
    let profile = match lane {
        PackLane::Native => CodecProfile::native(),
        _ => CodecProfile::whole_file(capacities),
    };
    match (has_base, base) {
        (true, Some(base)) => workspace.decompress_prefix(profile, frame, raw_length, base),
        (false, None) => workspace.decompress(profile, frame, raw_length),
        (true, None) => Err(StorageError::Integrity("prefix record without base bytes")),
        (false, Some(_)) => Err(StorageError::Integrity("FULL record with base bytes")),
    }
}

/// Returns every framed record of a group body, in ordinal order.
pub fn group_records(group: &[u8]) -> StorageResult<Vec<&[u8]>> {
    if group.len() < 4 {
        return Err(StorageError::Integrity("group framing"));
    }
    let count = u32::from_le_bytes(
        group[..4]
            .try_into()
            .map_err(|_| StorageError::Integrity("group record count"))?,
    ) as usize;
    let mut records = Vec::with_capacity(count.min(crate::policy::RECORD_COUNT_LIMIT));
    for ordinal in 0..count {
        records.push(framed_record(group, ordinal)?);
    }
    Ok(records)
}

/// Extracts one framed record from a group body.
pub fn framed_record(group: &[u8], ordinal: usize) -> StorageResult<&[u8]> {
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
