//! Versioned raw meanings under the filesystem operation marker.
use super::record::{Row, ROW_BYTES};
use crate::filesystem::path::PathName;
use crate::object::inode_leaf::{
    decode_inode_value, encode_inode_value, InodeValue, INODE_VALUE_BYTES,
};
use crate::{ConstructionRecordChange, ConstructionRecordExpected, ConstructionRecordKey};
use crate::{ContentError, ContentResult, ObjectId};

pub(super) const FRESH: u32 = 0x4653_0004;
pub(super) const ROW: u32 = FRESH + 1;
pub(super) const TOUCH: u32 = FRESH + 2;
pub(super) const WORK: u32 = FRESH + 3;
pub(super) const FRAME: u32 = FRESH + 4;
pub(super) const NODE: u32 = FRESH + 5;

pub(super) fn key(kind: u32, serial: u64) -> ConstructionRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&serial.to_be_bytes());
    ConstructionRecordKey { kind, key }
}
pub(super) fn serial(key: [u8; 32]) -> ContentResult<u64> {
    if key[..24].iter().any(|byte| *byte != 0) {
        return Err(ContentError::InvalidRecord("filesystem reference key"));
    }
    let serial = u64::from_be_bytes(
        key[24..]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    if serial == 0 {
        return Err(ContentError::InvalidRecord("filesystem reference key"));
    }
    Ok(serial)
}
pub(super) fn change(
    kind: u32,
    serial: u64,
    old: Option<Vec<u8>>,
    value: Option<Vec<u8>>,
) -> ConstructionRecordChange {
    ConstructionRecordChange {
        key: key(kind, serial),
        expected: old.map_or(
            ConstructionRecordExpected::Missing,
            ConstructionRecordExpected::ExactBytes,
        ),
        value,
    }
}
pub(super) fn row(bytes: &[u8], serial: u64) -> ContentResult<Row> {
    let raw: &[u8; ROW_BYTES] = bytes
        .try_into()
        .map_err(|_| ContentError::InvalidOrderingRecord("length"))?;
    let row = Row::decode(raw)?;
    if row.serial() != serial {
        return Err(ContentError::InvalidRecord("filesystem reference serial"));
    }
    Ok(row)
}
pub(super) fn membership(bytes: &[u8]) -> ContentResult<()> {
    if bytes != [1] {
        return Err(ContentError::InvalidRecord(
            "filesystem reference membership",
        ));
    }
    Ok(())
}

pub(super) struct Item {
    pub serial: u64,
    pub base: Option<InodeValue>,
}
impl Item {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(10 + self.base.map_or(0, |_| INODE_VALUE_BYTES));
        bytes.push(1);
        bytes.extend_from_slice(&self.serial.to_be_bytes());
        bytes.push(u8::from(self.base.is_some()));
        if let Some(base) = self.base {
            bytes.extend_from_slice(&encode_inode_value(base));
        }
        bytes
    }
    pub fn decode(bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() < 10 || bytes[0] != 1 {
            return Err(ContentError::InvalidRecord("filesystem release work"));
        }
        let serial = u64::from_be_bytes(
            bytes[1..9]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        );
        if serial == 0 {
            return Err(ContentError::InvalidRecord("filesystem release work"));
        }
        let base = match bytes[9] {
            0 if bytes.len() == 10 => None,
            1 if bytes.len() == 10 + INODE_VALUE_BYTES => Some(decode_inode_value(
                bytes[10..]
                    .try_into()
                    .map_err(|_| ContentError::UnexpectedEof)?,
            )?),
            _ => return Err(ContentError::InvalidRecord("filesystem release work")),
        };
        Ok(Self { serial, base })
    }
}

pub(super) struct Frame {
    pub serial: u64,
    pub root: ObjectId,
    pub accepted: bool,
    pub finished: bool,
    pub after: Option<PathName>,
}
impl Frame {
    pub fn encode(&self) -> Vec<u8> {
        let name = self.after.as_ref().map_or(&[][..], PathName::as_bytes);
        let mut bytes = Vec::with_capacity(45 + name.len());
        bytes.push(1);
        bytes.extend_from_slice(&self.serial.to_be_bytes());
        bytes.extend_from_slice(&self.root.to_bytes());
        bytes.extend_from_slice(&[u8::from(self.accepted), u8::from(self.finished)]);
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name);
        bytes
    }
    pub fn decode(bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() < 45 || bytes[0] != 1 || bytes[41] > 1 || bytes[42] > 1 {
            return Err(ContentError::InvalidRecord("filesystem release frame"));
        }
        let serial = u64::from_be_bytes(
            bytes[1..9]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        );
        let len = usize::from(u16::from_be_bytes(
            bytes[43..45]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        ));
        if serial == 0 || len > 255 || bytes.len() != 45 + len {
            return Err(ContentError::InvalidRecord("filesystem release frame"));
        }
        Ok(Self {
            serial,
            root: ObjectId::from_bytes(&bytes[9..41])?,
            accepted: bytes[41] == 1,
            finished: bytes[42] == 1,
            after: if len == 0 {
                None
            } else {
                Some(PathName::from_bytes(&bytes[45..])?)
            },
        })
    }
}
