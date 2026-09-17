//! FULL representations and the shared per-object raw payload view.
//!
//! A FULL record reconstructs its canonical object without any delta base. It may
//! be compressed: chunk and whole-file payloads carry a prefix-capable frame,
//! while mapping pages are stored as exact canonical bytes inside an ordinary
//! group. Nothing here selects a representation and nothing here retries: a failed
//! codec call is returned as a failure. A whole-file record whose planned compact
//! form cannot fit a normal pack is planned into the singleton lane instead, with
//! the same frame bytes and the same FULL/PREFIX choice.

use layerfs_content::file::mapping::{decode_chunk_payload, CHUNK_MAGIC};
use layerfs_content::{whole_file_payload, ObjectId, ObjectRole};

use crate::encoding::codec::{CodecProfile, CompressionWorkspace};
use crate::encoding::delta::record;
use crate::error::{StorageError, StorageResult};
use crate::pack::assemble::FULL_TAG;
use crate::pack::layout::{
    PackLane, DIRECTORY_ENTRY_LEN, HEADER_LEN, WHOLE_FILE_COMPACT_DROP, WHOLE_FILE_ENTRY_LEN,
};
use crate::policy::{StorageCapacities, CANONICAL_LIMIT};

/// One framed record ready to join a group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRecord {
    /// Lane the record belongs to.
    pub lane: PackLane,
    /// Record bytes in their lane's grammar.
    pub record: Vec<u8>,
    /// Canonical object length this record reconstructs.
    pub canonical_length: usize,
    /// Raw payload length, or the canonical length when the record is unframed.
    pub raw_length: usize,
    /// Direct physical base, absent for a FULL record.
    pub base: Option<ObjectId>,
}

impl EncodedRecord {
    /// Stored width of this record in its lane's grammar.
    pub fn width(&self) -> usize {
        self.record.len()
    }
}

/// Borrows the raw payload a role's records are framed against.
pub fn raw_payload(canonical: &[u8], role: ObjectRole) -> StorageResult<&[u8]> {
    match role {
        ObjectRole::WholeFile => whole_file_payload(canonical)?.ok_or(StorageError::Integrity(
            "whole-file role with a foreign payload",
        )),
        ObjectRole::Chunk => {
            let value = layerfs_content::object::codec::decode_bytes_object(canonical)?;
            if !value.starts_with(CHUNK_MAGIC) {
                return Err(StorageError::Integrity(
                    "chunk role with a non-chunk payload",
                ));
            }
            Ok(decode_chunk_payload(value)?)
        }
        ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch | ObjectRole::FileState => {
            Err(StorageError::Integrity("role is stored unframed"))
        }
        ObjectRole::InodeLeaf => Err(StorageError::Integrity(
            "pooled metadata leaf is stored by its own lane",
        )),
    }
}

/// Encodes the supported FULL record for one finalized canonical object.
pub fn encode_full(
    canonical: &[u8],
    role: ObjectRole,
    capacities: &StorageCapacities,
    workspace: &mut CompressionWorkspace,
) -> StorageResult<EncodedRecord> {
    encode_representation(canonical, role, capacities, workspace, None)
}

/// Encodes a PREFIX record against one direct base payload.
pub fn encode_prefix(
    canonical: &[u8],
    role: ObjectRole,
    base_id: ObjectId,
    base_raw: &[u8],
    capacities: &StorageCapacities,
    workspace: &mut CompressionWorkspace,
) -> StorageResult<EncodedRecord> {
    encode_representation(
        canonical,
        role,
        capacities,
        workspace,
        Some((base_id, base_raw)),
    )
}

fn encode_representation(
    canonical: &[u8],
    role: ObjectRole,
    capacities: &StorageCapacities,
    workspace: &mut CompressionWorkspace,
    prefix: Option<(ObjectId, &[u8])>,
) -> StorageResult<EncodedRecord> {
    if canonical.is_empty() || canonical.len() > CANONICAL_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "encoding.canonical_length",
            limit: CANONICAL_LIMIT as u64,
            actual: canonical.len() as u64,
        });
    }
    match PackLane::for_role(role) {
        PackLane::Ordinary => {
            if prefix.is_some() {
                return Err(StorageError::Integrity(
                    "ordinary lane representation has no prefix record",
                ));
            }
            let mut record = Vec::with_capacity(canonical.len() + 1);
            record.push(FULL_TAG);
            record.extend_from_slice(canonical);
            Ok(EncodedRecord {
                lane: PackLane::Ordinary,
                record,
                canonical_length: canonical.len(),
                raw_length: canonical.len(),
                base: None,
            })
        }
        PackLane::PooledMetadata => Err(StorageError::Integrity(
            "pooled metadata lane is encoded by its own module",
        )),
        PackLane::WholeFile | PackLane::Native => {
            let raw = raw_payload(canonical, role)?;
            let profile = match PackLane::for_role(role) {
                PackLane::WholeFile => CodecProfile::whole_file(capacities),
                _ => CodecProfile::native(),
            };
            if raw.is_empty() || raw.len() > profile.raw_limit() {
                return Err(StorageError::CapacityExceeded {
                    what: match PackLane::for_role(role) {
                        PackLane::WholeFile => "encoding.whole_file_raw",
                        _ => "encoding.chunk_raw",
                    },
                    limit: profile.raw_limit() as u64,
                    actual: raw.len() as u64,
                });
            }
            let frame = match prefix {
                Some((_, base_raw)) => workspace.compress_prefix(profile, raw, base_raw)?,
                None => workspace.compress(profile, raw)?,
            };
            let base = prefix.map(|(id, _)| id);
            let (lane, record) = plan_lane(role, raw.len(), base, &frame, capacities)?;
            Ok(EncodedRecord {
                lane,
                record,
                canonical_length: canonical.len(),
                raw_length: raw.len(),
                base,
            })
        }
        PackLane::Singleton => Err(StorageError::Integrity("singleton lane is planned")),
    }
}

/// Chooses the lane and frames `frame` in that lane's grammar.
///
/// The compact whole-file lane is preferred whenever the complete planned record
/// fits a normal pack; otherwise the record is planned into the singleton lane,
/// which is bounded by its own pack limit. The frame bytes are reused either way,
/// so the codec runs exactly once per representation.
fn plan_lane(
    role: ObjectRole,
    raw_length: usize,
    base: Option<ObjectId>,
    frame: &[u8],
    capacities: &StorageCapacities,
) -> StorageResult<(PackLane, Vec<u8>)> {
    let lane = PackLane::for_role(role);
    if lane != PackLane::WholeFile {
        return Ok((lane, record::encode(lane, raw_length, base, frame)?));
    }
    let compact = record::encode(PackLane::WholeFile, raw_length, base, frame)?;
    let body = compact
        .len()
        .checked_sub(WHOLE_FILE_COMPACT_DROP)
        .ok_or(StorageError::Integrity("compact record width"))?;
    let contribution = HEADER_LEN + WHOLE_FILE_ENTRY_LEN + body;
    if contribution <= capacities.pack_limit {
        return Ok((PackLane::WholeFile, compact));
    }
    // The compact record lost. It is released before the singleton record is
    // built, so the two candidate records are never live together: at the largest
    // accepted record that overlap would be two ~16 MiB allocations for one
    // object, and only one of them is written.
    drop(compact);
    let record = record::encode(PackLane::Singleton, raw_length, base, frame)?;
    let contribution = HEADER_LEN + DIRECTORY_ENTRY_LEN + record.len() + 8;
    if contribution > capacities.singleton_pack_limit {
        return Err(StorageError::CapacityExceeded {
            what: "encoding.singleton_pack",
            limit: capacities.singleton_pack_limit as u64,
            actual: contribution as u64,
        });
    }
    Ok((PackLane::Singleton, record))
}

/// Largest group body the owner accepts for one lane before placement decides.
pub fn lane_body_limit(lane: PackLane, capacities: &StorageCapacities) -> usize {
    match lane {
        PackLane::WholeFile => capacities.pack_limit,
        PackLane::Native => capacities.group_limit,
        PackLane::Ordinary => capacities.group_limit,
        PackLane::PooledMetadata => capacities.metadata_group_limit,
        PackLane::Singleton => capacities.singleton_pack_limit,
    }
}
