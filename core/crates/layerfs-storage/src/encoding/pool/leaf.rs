//! Pooled leaf records: the physical form of one inode leaf page.
//!
//! A pooled leaf keeps the canonical prefix and replaces every 73-byte value with
//! a 4-byte ordinal into a value group. The record grammar is the ordinary lane's
//! record framing with two pooled tags: tag zero carries the physical body, tag
//! one carries a base identity, the output length, an instruction count and a
//! COPY/INSERT program. These are the reference's own record tags, and the role
//! recorded with the locator decides which interpretation applies: an `InodeLeaf`
//! locator always carries a pooled record, so tag zero is a pooled body there and
//! a plain canonical object everywhere else.
//!
//! The physical body is `prefix || (serial, ordinal) *`, exactly the layout the C1
//! inode-leaf grammar derives, so both components agree on one byte layout.

use layerfs_content::inode_leaf::{
    decode_pooled_body, pooled_physical_length, PooledRow, POOLED_PREFIX_BYTES,
};
use layerfs_content::ObjectId;

use crate::error::{StorageError, StorageResult};
use crate::policy::METADATA_RECORD_LIMIT;

/// Record tag of a pooled leaf stored in full.
pub const POOLED_FULL_TAG: u8 = 0;
/// Record tag of a pooled leaf stored against one direct base.
pub const POOLED_DELTA_TAG: u8 = 1;

/// One parsed pooled leaf record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PooledRecord<'a> {
    /// A complete physical body.
    Full(&'a [u8]),
    /// Instructions against one direct base.
    Delta {
        /// Direct base.
        base: ObjectId,
        /// Physical body length the program produces.
        output_length: usize,
        /// Instruction count.
        count: usize,
        /// Instruction program.
        instructions: &'a [u8],
    },
}

/// Encodes the full pooled record of one physical body.
pub fn encode_full(body: &[u8]) -> StorageResult<Vec<u8>> {
    if body.is_empty() || body.len() > METADATA_RECORD_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "pool.record",
            limit: METADATA_RECORD_LIMIT as u64,
            actual: body.len() as u64,
        });
    }
    decode_pooled_body(body)?;
    let mut record = Vec::with_capacity(body.len() + 1);
    record.push(POOLED_FULL_TAG);
    record.extend_from_slice(body);
    Ok(record)
}

/// Parses one pooled leaf record.
pub fn parse(record: &[u8]) -> StorageResult<PooledRecord<'_>> {
    match record.first() {
        Some(&POOLED_FULL_TAG) => {
            let body = &record[1..];
            if body.is_empty() || body.len() > METADATA_RECORD_LIMIT {
                return Err(StorageError::Integrity("pooled record width"));
            }
            decode_pooled_body(body)?;
            Ok(PooledRecord::Full(body))
        }
        Some(&POOLED_DELTA_TAG) => {
            if record.len() < 41 || record.len() > METADATA_RECORD_LIMIT {
                return Err(StorageError::Integrity("pooled delta record width"));
            }
            let base = ObjectId::from_bytes(&record[1..33])?;
            let output_length = read_u32(record, 33)?;
            let count = read_u32(record, 37)?;
            let instructions = &record[41..];
            crate::encoding::pool::delta::validate(instructions, count, output_length)?;
            Ok(PooledRecord::Delta {
                base,
                output_length,
                count,
                instructions,
            })
        }
        _ => Err(StorageError::Integrity("pooled record tag")),
    }
}

/// Physical length of the body stored at one pooled locator.
pub fn physical_length(canonical_length: usize) -> StorageResult<usize> {
    Ok(pooled_physical_length(canonical_length)?)
}

/// Canonical leaf length a physical body of `rows` rows re-expands to.
pub fn canonical_length(rows: usize) -> StorageResult<usize> {
    if rows == 0 || rows > layerfs_content::inode_leaf::MAXIMUM_LEAF_ROWS {
        return Err(StorageError::Integrity("pooled row count"));
    }
    POOLED_PREFIX_BYTES
        .checked_add(
            rows.checked_mul(layerfs_content::inode_leaf::LEAF_ROW_BYTES)
                .ok_or(StorageError::Integrity("pooled leaf length"))?,
        )
        .ok_or(StorageError::Integrity("pooled leaf length"))
}

/// Distinct ordinals a body refers to, in first-encounter order.
pub fn ordinals(body: &[u8]) -> StorageResult<Vec<u32>> {
    let (_, rows) = decode_pooled_body(body)?;
    let mut ordinals: Vec<u32> = Vec::new();
    for row in &rows {
        if !ordinals.contains(&row.ordinal) {
            ordinals.push(row.ordinal);
        }
    }
    Ok(ordinals)
}

/// Rows of a pooled body.
pub fn rows(body: &[u8]) -> StorageResult<Vec<PooledRow>> {
    Ok(decode_pooled_body(body)?.1)
}

fn read_u32(bytes: &[u8], offset: usize) -> StorageResult<usize> {
    let end = offset
        .checked_add(4)
        .ok_or(StorageError::Integrity("pooled field"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or(StorageError::Integrity("pooled field"))?;
    Ok(u32::from_le_bytes(
        slice
            .try_into()
            .map_err(|_| StorageError::Integrity("pooled field"))?,
    ) as usize)
}
