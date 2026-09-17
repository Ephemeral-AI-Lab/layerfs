//! Value groups: the exact physical representation of pooled metadata values.
//!
//! A group is the ordinary group framing over the canonical value objects, hashed
//! *before* compression so the catalogue digest authenticates the decoded body, and
//! then stored compressed when the retained rule says the frame is smaller. The
//! group bound is 16 KiB, which fixes 165 values per group for this profile.

use layerfs_content::inode_leaf::{decode_pooled_value, encode_pooled_value};
use layerfs_content::ObjectId;

use crate::encoding::codec::CompressionWorkspace;
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::{EncodedGroup, GroupCodec, PackLane};
use crate::pack::{assemble::frame_group_bounded, assemble::framed_length};
use crate::policy::{METADATA_GROUP_LIMIT, VALUES_PER_GROUP};

/// One built value group.
#[derive(Clone, Debug)]
pub struct BuiltGroup {
    /// Decoded group body.
    pub body: Vec<u8>,
    /// Digest recorded in the catalogue.
    pub digest: ObjectId,
    /// Placement-ready group.
    pub group: EncodedGroup,
    /// Values stored in this group.
    pub count: usize,
}

/// Builds one value group from canonical value objects.
pub fn build(
    values: &[Vec<u8>],
    workspace: &mut CompressionWorkspace,
) -> StorageResult<BuiltGroup> {
    if values.is_empty() || values.len() > VALUES_PER_GROUP {
        return Err(StorageError::Integrity("value group count"));
    }
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        decode_pooled_value(value)?;
        let mut record = Vec::with_capacity(value.len() + 1);
        record.push(crate::pack::assemble::FULL_TAG);
        record.extend_from_slice(value);
        records.push(record);
    }
    let body = frame_group_bounded(&records, METADATA_GROUP_LIMIT)?;
    let digest = ObjectId::for_bytes(&body);
    // The body is copied only when the frame is not smaller than it: the stored
    // bytes are the frame when there is one, and the body itself otherwise.
    let (encoded, codec) = match crate::encoding::codec::compress_group_body(workspace, &body)? {
        Some(frame) => (frame, GroupCodec::Zstandard),
        None => (body.clone(), GroupCodec::Raw),
    };
    Ok(BuiltGroup {
        group: EncodedGroup {
            decoded_length: framed_length(&records)?,
            bytes: encoded,
            records: values.len(),
            codec,
        },
        body,
        digest,
        count: values.len(),
    })
}

/// Authenticates a decoded group body against its catalogue digest.
pub fn authenticate(body: &[u8], digest: ObjectId) -> StorageResult<()> {
    if body.is_empty() || body.len() > METADATA_GROUP_LIMIT {
        return Err(StorageError::Integrity("value group body length"));
    }
    if ObjectId::for_bytes(body) != digest {
        return Err(StorageError::Integrity("value group identity"));
    }
    Ok(())
}

/// Splits an authenticated body into its canonical value objects.
pub fn decode(body: &[u8], expected: usize) -> StorageResult<Vec<Vec<u8>>> {
    let records = crate::encoding::decode::group_records(body)?;
    if records.len() != expected || expected == 0 || expected > VALUES_PER_GROUP {
        return Err(StorageError::Integrity("value group record count"));
    }
    let mut values = Vec::with_capacity(records.len());
    for record in records {
        if record.first() != Some(&crate::pack::assemble::FULL_TAG) {
            return Err(StorageError::Integrity("value group FULL role"));
        }
        decode_pooled_value(&record[1..])?;
        values.push(record[1..].to_vec());
    }
    Ok(values)
}

/// Wraps one inode value as a canonical pooled value object.
pub fn canonical_value(value: &[u8; 73]) -> StorageResult<Vec<u8>> {
    Ok(encode_pooled_value(value)?)
}

/// Lane value groups are stored in.
pub const fn lane() -> PackLane {
    PackLane::PooledMetadata
}
