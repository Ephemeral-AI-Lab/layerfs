//! Bounded acknowledgement witness; reuse checks exact summary and fill context.
use super::references::Summary;
use crate::{ContentError, ContentResult, ObjectId};

pub(super) fn encode(id: ObjectId, summary: Summary, nonroot_valid: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(51);
    bytes.push(1);
    bytes.extend_from_slice(&id.to_bytes());
    bytes.extend_from_slice(&summary.bytes.to_be_bytes());
    bytes.extend_from_slice(&summary.extents.to_be_bytes());
    bytes.push(summary.level);
    bytes.push(u8::from(nonroot_valid));
    bytes
}

pub(super) fn check(bytes: &[u8], summary: Summary, root: bool) -> ContentResult<ObjectId> {
    if bytes.len() != 51 || bytes[0] != 1 || bytes[50] > 1 {
        return Err(ContentError::InvalidRecord("resolved mapping witness"));
    }
    let logical = u64::from_be_bytes(
        bytes[33..41]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    let extents = u64::from_be_bytes(
        bytes[41..49]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    if logical != summary.bytes || extents != summary.extents || bytes[49] != summary.level {
        return Err(ContentError::InvalidRecord("resolved mapping summary"));
    }
    if !root && bytes[50] == 0 {
        return Err(ContentError::NonCanonicalPagePartition);
    }
    ObjectId::from_bytes(&bytes[1..33])
}
