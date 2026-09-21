//! Delta record framing for the compact whole-file and native chunk lanes.
//!
//! A record is either FULL (no base) or PREFIX (exactly one direct base). The
//! base identity is part of the record, and therefore part of its cost, so a
//! selection compares complete record costs rather than frame lengths alone.
//! Both grammars are shared by the encoder here and by the reconstruction path,
//! so a record that cannot be parsed cannot be selected or read.
//!
//! The base identity is also the **only** record of a dependency edge: it is not
//! duplicated in a locator column, and `stored_base` is the single reader of it
//! for every walk.

use layerfs_content::{ObjectId, ObjectRole};

use crate::error::{StorageError, StorageResult};
use crate::pack::layout::{PackLane, WHOLE_FILE_COMPACT_DROP};

/// Record tag of a representation without a base.
pub const FULL_TAG: u8 = 0;
/// Record tag of a representation against one direct base.
pub const PREFIX_TAG: u8 = 1;
/// Record tag of a payload stored verbatim, with no base and no codec frame.
///
/// The tag is the whole cost of the form: the byte is already there for
/// [`FULL_TAG`] and [`PREFIX_TAG`], and a stored payload needs no length field the
/// lane did not already carry. A stored record is always FULL - storing a payload
/// verbatim says nothing about any base, and a record that carries one would
/// claim a dependency edge its bytes do not use.
pub const STORED_TAG: u8 = 2;

/// True when `tag` names a payload stored verbatim rather than as a codec frame.
pub const fn is_stored(tag: u8) -> bool {
    tag == STORED_TAG
}

/// Bytes of the compact whole-file framing that the lane drops at assembly.
const COMPACT_DROP: usize = WHOLE_FILE_COMPACT_DROP;
/// Width of one canonical identity.
const ID_BYTES: usize = 32;

/// One parsed lane record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParsedRecord<'a> {
    /// Representation tag.
    pub tag: u8,
    /// Raw payload length the frame decodes to.
    pub raw_length: usize,
    /// Direct base, present exactly when the tag is [`PREFIX_TAG`].
    pub base: Option<ObjectId>,
    /// Compressed frame.
    pub frame: &'a [u8],
}

/// Exact stored width of one record of `lane` with the given parts.
pub fn record_width(lane: PackLane, base: Option<ObjectId>, frame_length: usize) -> usize {
    match lane {
        PackLane::WholeFile => 1 + 8 + usize::from(base.is_some()) * ID_BYTES + frame_length,
        PackLane::Native => 1 + 4 + usize::from(base.is_some()) * ID_BYTES + frame_length,
        PackLane::Ordinary | PackLane::PooledMetadata | PackLane::Singleton => {
            1 + 8 + usize::from(base.is_some()) * ID_BYTES + frame_length
        }
    }
}

/// Encodes one whole-file or native lane record.
pub fn encode(
    lane: PackLane,
    raw_length: usize,
    base: Option<ObjectId>,
    frame: &[u8],
) -> StorageResult<Vec<u8>> {
    let tag = match base {
        Some(_) => PREFIX_TAG,
        None => FULL_TAG,
    };
    encode_tagged(lane, tag, raw_length, base, frame)
}

/// Encodes one payload record whose bytes are the payload itself.
///
/// The stored form carries no base: the payload is stored as it stands, so there
/// is no frame to decode against one.
pub fn encode_stored(lane: PackLane, raw_length: usize, payload: &[u8]) -> StorageResult<Vec<u8>> {
    if payload.len() != raw_length {
        return Err(StorageError::Integrity("stored record width"));
    }
    encode_tagged(lane, STORED_TAG, raw_length, None, payload)
}

/// Encodes one whole-file or native lane record under an explicit tag.
pub fn encode_tagged(
    lane: PackLane,
    tag: u8,
    raw_length: usize,
    base: Option<ObjectId>,
    frame: &[u8],
) -> StorageResult<Vec<u8>> {
    if frame.is_empty() || raw_length == 0 {
        return Err(StorageError::Integrity("record framing"));
    }
    if is_stored(tag) {
        if base.is_some() || frame.len() != raw_length {
            return Err(StorageError::Integrity("stored record framing"));
        }
    } else if tag != FULL_TAG && tag != PREFIX_TAG {
        return Err(StorageError::Integrity("record tag"));
    } else if (tag == PREFIX_TAG) != base.is_some() {
        return Err(StorageError::Integrity("record base tag"));
    }
    let mut record = Vec::with_capacity(record_width(lane, base, frame.len()));
    record.push(tag);
    match lane {
        PackLane::WholeFile
        | PackLane::Ordinary
        | PackLane::PooledMetadata
        | PackLane::Singleton => {
            record.extend_from_slice(
                &u32::try_from(raw_length)
                    .map_err(|_| StorageError::Integrity("record raw length"))?
                    .to_le_bytes(),
            );
            record.extend_from_slice(
                &u32::try_from(frame.len())
                    .map_err(|_| StorageError::Integrity("record frame length"))?
                    .to_le_bytes(),
            );
        }
        PackLane::Native => {
            record.extend_from_slice(
                &u32::try_from(raw_length)
                    .map_err(|_| StorageError::Integrity("record raw length"))?
                    .to_le_bytes(),
            );
        }
    }
    if let Some(base) = base {
        record.extend_from_slice(base.as_bytes());
    }
    record.extend_from_slice(frame);
    Ok(record)
}

/// Parses one record as it is stored in `lane`.
///
/// `canonical_length` is required by the compact whole-file lane, which drops its
/// two little-endian length fields at assembly and re-derives them.
pub fn parse(
    lane: PackLane,
    record: &[u8],
    canonical_length: usize,
) -> StorageResult<ParsedRecord<'_>> {
    match lane {
        PackLane::WholeFile => parse_compact(record, canonical_length),
        PackLane::Native => parse_native(record),
        PackLane::Ordinary => parse_ordinary(record),
        PackLane::PooledMetadata | PackLane::Singleton => parse_framed(record),
    }
}

fn parse_ordinary(record: &[u8]) -> StorageResult<ParsedRecord<'_>> {
    if record.first() != Some(&FULL_TAG) {
        return Err(StorageError::Integrity("ordinary record tag is not FULL"));
    }
    Ok(ParsedRecord {
        tag: FULL_TAG,
        raw_length: record.len().saturating_sub(1),
        base: None,
        frame: &record[1..],
    })
}

/// Parses a compact whole-file record after its framing bytes were dropped.
fn parse_compact(record: &[u8], canonical_length: usize) -> StorageResult<ParsedRecord<'_>> {
    let raw_length = canonical_length
        .checked_sub(crate::policy::WHOLE_FILE_CANONICAL_OVERHEAD)
        .ok_or(StorageError::Integrity("compact canonical length"))?;
    if raw_length == 0 {
        return Err(StorageError::Integrity("compact raw length"));
    }
    let tag = *record
        .first()
        .ok_or(StorageError::Integrity("compact record width"))?;
    let (base, start) = match tag {
        FULL_TAG | STORED_TAG => (None, 1),
        PREFIX_TAG => (
            Some(ObjectId::from_bytes(
                record
                    .get(1..1 + ID_BYTES)
                    .ok_or(StorageError::Integrity("compact base identity"))?,
            )?),
            1 + ID_BYTES,
        ),
        _ => return Err(StorageError::Integrity("compact record tag")),
    };
    let frame = record
        .get(start..)
        .ok_or(StorageError::Integrity("compact record frame"))?;
    if frame.is_empty() {
        return Err(StorageError::Integrity("compact record frame"));
    }
    Ok(ParsedRecord {
        tag,
        raw_length,
        base,
        frame,
    })
}

/// Parses a native chunk record.
pub fn parse_native(record: &[u8]) -> StorageResult<ParsedRecord<'_>> {
    if record.len() < 5 {
        return Err(StorageError::Integrity("native record framing"));
    }
    let raw_length = u32::from_le_bytes(
        record[1..5]
            .try_into()
            .map_err(|_| StorageError::Integrity("native raw length"))?,
    ) as usize;
    let (base, start) = match record[0] {
        FULL_TAG | STORED_TAG => (None, 5),
        PREFIX_TAG => (
            Some(ObjectId::from_bytes(
                record
                    .get(5..5 + ID_BYTES)
                    .ok_or(StorageError::Integrity("native base identity"))?,
            )?),
            5 + ID_BYTES,
        ),
        _ => return Err(StorageError::Integrity("native record tag")),
    };
    let frame = record
        .get(start..)
        .ok_or(StorageError::Integrity("native record frame"))?;
    if frame.is_empty() || raw_length == 0 {
        return Err(StorageError::Integrity("native record framing"));
    }
    Ok(ParsedRecord {
        tag: record[0],
        raw_length,
        base,
        frame,
    })
}

/// Parses a framed record of the pooled metadata or singleton lane.
fn parse_framed(record: &[u8]) -> StorageResult<ParsedRecord<'_>> {
    if record.len() < 9 {
        return Err(StorageError::Integrity("framed record width"));
    }
    let raw_length = u32::from_le_bytes(
        record[1..5]
            .try_into()
            .map_err(|_| StorageError::Integrity("framed raw length"))?,
    ) as usize;
    let frame_length = u32::from_le_bytes(
        record[5..9]
            .try_into()
            .map_err(|_| StorageError::Integrity("framed frame length"))?,
    ) as usize;
    let (base, start) = match record[0] {
        FULL_TAG | STORED_TAG => (None, 9),
        PREFIX_TAG => (
            Some(ObjectId::from_bytes(
                record
                    .get(9..9 + ID_BYTES)
                    .ok_or(StorageError::Integrity("framed base identity"))?,
            )?),
            9 + ID_BYTES,
        ),
        _ => return Err(StorageError::Integrity("framed record tag")),
    };
    let frame = record
        .get(start..)
        .ok_or(StorageError::Integrity("framed record frame"))?;
    if frame.is_empty() || frame.len() != frame_length || raw_length == 0 {
        return Err(StorageError::Integrity("framed record frame"));
    }
    Ok(ParsedRecord {
        tag: record[0],
        raw_length,
        base,
        frame,
    })
}

/// The direct base identity a stored record carries, or `None` when it is FULL.
///
/// This is the **only** source of a dependency edge: there is no base column, so
/// a caller that wants to walk a chain reads the record. The answer is derived
/// from the same grammar the decoder uses, so a record whose framing cannot be
/// parsed is an integrity failure here rather than a chain that silently ends.
///
/// An `Ordinary` locator is ambiguous by grammar alone: the lane holds both plain
/// canonical objects, whose records are always FULL, and pooled inode leaves,
/// whose records are pooled ones. The role recorded with the locator decides, the
/// same way it decides for the decoder.
pub fn stored_base(
    lane: PackLane,
    record: &[u8],
    canonical_length: usize,
    role: ObjectRole,
) -> StorageResult<Option<ObjectId>> {
    if lane == PackLane::Ordinary && role == ObjectRole::InodeLeaf {
        return Ok(match crate::encoding::pool::leaf::parse(record)? {
            crate::encoding::pool::leaf::PooledRecord::Full(_) => None,
            crate::encoding::pool::leaf::PooledRecord::Delta { base, .. } => Some(base),
        });
    }
    Ok(parse(lane, record, canonical_length)?.base)
}

/// True when `record` was assembled with its compact framing bytes dropped.
pub const fn compact_framing_is_dropped(lane: PackLane) -> bool {
    matches!(lane, PackLane::WholeFile) && COMPACT_DROP > 0
}
