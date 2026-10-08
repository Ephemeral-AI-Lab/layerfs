//! Filesystem meanings are disjoint from the file editor's kind domains.
use crate::filesystem::root::profile_id;
use crate::filesystem::rows::view::OperationInput;
use crate::{ConstructionRecordChange, ConstructionRecordExpected, ConstructionRecordKey};
use crate::{ContentError, ContentResult, ObjectId};

pub(super) const CONTEXT: u32 = 0x4653_0000;
pub(super) const PARENT: u32 = CONTEXT + 1;
pub(super) const ROOT: u32 = CONTEXT + 2;
pub(super) const COUNT: u32 = CONTEXT + 3;
/// Whole-root qualification pass context, inode rows and directory queue.
/// Kinds +4..=+9 belong to the indexed reference rows.
pub(crate) const QUALIFY_CONTEXT: u32 = CONTEXT + 0x10;
pub(crate) const QUALIFY_INODE: u32 = CONTEXT + 0x11;
pub(crate) const QUALIFY_QUEUE: u32 = CONTEXT + 0x12;
/// Validation topology evidence: placements, territory marks, rooted marks, the
/// territory queue and scanned parents. Kinds +0x20..=+0x2F are reserved for it.
pub(crate) const TOPOLOGY_PLACED: u32 = CONTEXT + 0x20;
pub(crate) const TOPOLOGY_TERRITORY: u32 = CONTEXT + 0x21;
pub(crate) const TOPOLOGY_ROOTED: u32 = CONTEXT + 0x22;
pub(crate) const TOPOLOGY_QUEUE: u32 = CONTEXT + 0x23;
pub(crate) const TOPOLOGY_SCANNED: u32 = CONTEXT + 0x24;
pub(super) const WINDOW: usize = 65_536;

pub(crate) fn key(kind: u32, serial: u64) -> ConstructionRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&serial.to_be_bytes());
    ConstructionRecordKey { kind, key }
}
/// The serial a full enumerated key addresses; any other key shape is refused.
pub(crate) fn serial(key: [u8; 32]) -> ContentResult<u64> {
    if key[..24].iter().any(|byte| *byte != 0) {
        return Err(ContentError::InvalidRecord("filesystem state key"));
    }
    let serial = u64::from_be_bytes(
        key[24..]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    if serial == 0 {
        return Err(ContentError::InvalidRecord("filesystem state key"));
    }
    Ok(serial)
}
pub(super) fn context(input: &dyn OperationInput) -> Vec<u8> {
    // The base-presence flag distinguishes None from a full all-zero identity.
    let mut value = Vec::with_capacity(106);
    value.extend_from_slice(&[1, u8::from(input.base().is_some())]);
    value.extend_from_slice(&input.base().map_or([0; 32], |root| root.0.to_bytes()));
    value.extend_from_slice(&input.scope().object().to_bytes());
    value.extend_from_slice(&profile_id().to_bytes());
    value.extend_from_slice(&input.root_serial().to_be_bytes());
    value
}
pub(crate) fn change(
    kind: u32,
    serial: u64,
    old: Option<Vec<u8>>,
    value: Vec<u8>,
) -> ConstructionRecordChange {
    ConstructionRecordChange {
        key: key(kind, serial),
        expected: old.map_or(
            ConstructionRecordExpected::Missing,
            ConstructionRecordExpected::ExactBytes,
        ),
        value: Some(value),
    }
}
pub(super) fn parent(bytes: &[u8]) -> ContentResult<bool> {
    match bytes {
        [1, 0] => Ok(false),
        [1, 1] => Ok(true),
        _ => Err(ContentError::InvalidRecord("filesystem parent state")),
    }
}
pub(super) fn count(bytes: &[u8]) -> ContentResult<u64> {
    if bytes.len() != 9 || bytes[0] != 1 {
        return Err(ContentError::InvalidRecord("filesystem count state"));
    }
    Ok(u64::from_be_bytes(
        bytes[1..]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    ))
}
pub(super) fn encode_count(count: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(9);
    bytes.push(1);
    bytes.extend_from_slice(&count.to_be_bytes());
    bytes
}
pub(super) fn root(bytes: &[u8]) -> ContentResult<ObjectId> {
    if bytes.len() != 33 || bytes[0] != 1 {
        return Err(ContentError::InvalidRecord("filesystem rebuilt root state"));
    }
    ObjectId::from_bytes(&bytes[1..])
}
pub(super) fn encode_root(root: ObjectId) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(33);
    bytes.push(1);
    bytes.extend_from_slice(&root.to_bytes());
    bytes
}
