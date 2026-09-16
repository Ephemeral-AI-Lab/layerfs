//! Supported FULL representations.
//!
//! A FULL record reconstructs its canonical object without any delta base. It may
//! be compressed: chunk and whole-file payloads carry a prefix-capable frame,
//! while mapping pages are stored as exact canonical bytes inside an ordinary
//! group. Nothing here produces a DELTA, and a failed codec call is returned as a
//! failure rather than selecting another representation.

use layerfs_content::file::mapping::{decode_chunk_payload, CHUNK_MAGIC};
use layerfs_content::{whole_file_payload, ObjectRole};

use crate::encoding::codec::{CodecProfile, CompressionWorkspace};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::{PackLane, WHOLE_FILE_COMPACT_DROP};
use crate::policy::{CANONICAL_LIMIT, CHUNK_RAW_LIMIT, WHOLE_FILE_RAW_LIMIT};

/// One framed record ready to join a group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullRecord {
    /// Lane the record belongs to.
    pub lane: PackLane,
    /// Record bytes in their lane's grammar.
    pub record: Vec<u8>,
    /// Canonical object length this record reconstructs.
    pub canonical_length: usize,
    /// Raw payload length, or the canonical length when the record is unframed.
    pub raw_length: usize,
}

/// Encodes the supported FULL record for one finalized canonical object.
pub fn encode_full(
    canonical: &[u8],
    role: ObjectRole,
    workspace: &mut CompressionWorkspace,
) -> StorageResult<FullRecord> {
    if canonical.is_empty() || canonical.len() > CANONICAL_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "encoding.canonical_length",
            limit: CANONICAL_LIMIT as u64,
            actual: canonical.len() as u64,
        });
    }
    match PackLane::for_role(role) {
        PackLane::WholeFile => encode_whole_file(canonical, workspace),
        PackLane::Native => encode_chunk(canonical, workspace),
        PackLane::Ordinary => Ok(FullRecord {
            lane: PackLane::Ordinary,
            record: ordinary_record(canonical),
            canonical_length: canonical.len(),
            raw_length: canonical.len(),
        }),
    }
}

fn ordinary_record(canonical: &[u8]) -> Vec<u8> {
    let mut record = Vec::with_capacity(canonical.len() + 1);
    record.push(crate::pack::assemble::FULL_TAG);
    record.extend_from_slice(canonical);
    record
}

fn encode_whole_file(
    canonical: &[u8],
    workspace: &mut CompressionWorkspace,
) -> StorageResult<FullRecord> {
    let raw = whole_file_payload(canonical)?.ok_or(StorageError::Integrity(
        "whole-file role with a non-whole-file payload",
    ))?;
    if raw.is_empty() || raw.len() > WHOLE_FILE_RAW_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "encoding.whole_file_raw",
            limit: WHOLE_FILE_RAW_LIMIT as u64,
            actual: raw.len() as u64,
        });
    }
    let frame = workspace.compress(CodecProfile::Small, raw)?;
    let mut record = Vec::with_capacity(WHOLE_FILE_COMPACT_DROP + 1 + frame.len());
    record.push(crate::pack::assemble::FULL_TAG);
    record.extend_from_slice(
        &u32::try_from(raw.len())
            .map_err(|_| StorageError::Integrity("whole-file raw length"))?
            .to_le_bytes(),
    );
    record.extend_from_slice(
        &u32::try_from(frame.len())
            .map_err(|_| StorageError::Integrity("whole-file frame length"))?
            .to_le_bytes(),
    );
    record.extend_from_slice(&frame);
    Ok(FullRecord {
        lane: PackLane::WholeFile,
        record,
        canonical_length: canonical.len(),
        raw_length: raw.len(),
    })
}

fn encode_chunk(
    canonical: &[u8],
    workspace: &mut CompressionWorkspace,
) -> StorageResult<FullRecord> {
    let value = layerfs_content::object::codec::decode_bytes_object(canonical)?;
    let raw = if value.starts_with(CHUNK_MAGIC) {
        decode_chunk_payload(value)?
    } else {
        return Err(StorageError::Integrity(
            "chunk role with a non-chunk payload",
        ));
    };
    if raw.is_empty() || raw.len() > CHUNK_RAW_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "encoding.chunk_raw",
            limit: CHUNK_RAW_LIMIT as u64,
            actual: raw.len() as u64,
        });
    }
    let frame = workspace.compress(CodecProfile::Native, raw)?;
    let mut record = Vec::with_capacity(5 + frame.len());
    record.push(crate::pack::assemble::FULL_TAG);
    record.extend_from_slice(
        &u32::try_from(raw.len())
            .map_err(|_| StorageError::Integrity("chunk raw length"))?
            .to_le_bytes(),
    );
    record.extend_from_slice(&frame);
    Ok(FullRecord {
        lane: PackLane::Native,
        record,
        canonical_length: canonical.len(),
        raw_length: raw.len(),
    })
}
