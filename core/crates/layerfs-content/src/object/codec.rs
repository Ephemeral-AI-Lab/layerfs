//! Canonical object envelope: framing, checked decoding and one-pass encoding.
//!
//! The envelope is `LFSO`, a one-byte kind, a big-endian payload length and a
//! big-endian value length followed by the value. Encoding writes the final
//! allocation once; decoding borrows the value out of the supplied bytes and
//! rejects every length, kind and trailing-byte violation it finds.

use std::io::Write;

use crate::error::{ContentError, ContentResult};
use crate::policy::{MAX_CANONICAL_OBJECT_BYTES, MAX_OBJECT_FIELD_BYTES};

/// Canonical envelope magic.
pub const OBJECT_MAGIC: [u8; 4] = *b"LFSO";
/// Bytes-role object kind tag.
pub const BYTES_KIND: u8 = 1;
/// Envelope header width in bytes.
pub const HEADER_LEN: usize = 9;

/// Largest accepted payload, `16 MiB` minus the envelope header.
pub const MAX_PAYLOAD_BYTES: usize = MAX_CANONICAL_OBJECT_BYTES - HEADER_LEN;

/// Length prefix the canonical value carries, in bytes.
pub const VALUE_LEN_BYTES: usize = 4;

/// Canonical object width for a value of `value_len` bytes.
pub const fn canonical_len(value_len: usize) -> ContentResult<usize> {
    if value_len > MAX_OBJECT_FIELD_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAX_OBJECT_FIELD_BYTES,
            actual: value_len,
        });
    }
    let payload = match value_len.checked_add(VALUE_LEN_BYTES) {
        Some(payload) => payload,
        None => return Err(ContentError::LengthOverflow),
    };
    match HEADER_LEN.checked_add(payload) {
        Some(total) if total <= MAX_CANONICAL_OBJECT_BYTES => Ok(total),
        _ => Err(ContentError::ObjectLimitExceeded {
            limit: MAX_CANONICAL_OBJECT_BYTES,
            actual: value_len,
        }),
    }
}

/// Writes one canonical bytes-role object into `output`.
///
/// The caller sizes `output` from [`canonical_len`] and writes exactly once, so
/// no intermediate allocation of the same bytes exists.
pub fn encode_bytes_object_to<W: Write>(value: &[u8], writer: &mut W) -> ContentResult<()> {
    canonical_len(value.len())?;
    let payload_len = u32::try_from(value.len() + VALUE_LEN_BYTES).map_err(|_| {
        ContentError::ObjectLimitExceeded {
            limit: MAX_PAYLOAD_BYTES,
            actual: value.len(),
        }
    })?;
    let value_len = u32::try_from(value.len()).map_err(|_| ContentError::ObjectLimitExceeded {
        limit: MAX_PAYLOAD_BYTES,
        actual: value.len(),
    })?;
    write(writer, &OBJECT_MAGIC)?;
    write(writer, &[BYTES_KIND])?;
    write(writer, &payload_len.to_be_bytes())?;
    write(writer, &value_len.to_be_bytes())?;
    write(writer, value)
}

/// Encodes one canonical bytes-role object into a fresh allocation.
pub fn encode_bytes_object(value: &[u8]) -> ContentResult<Vec<u8>> {
    let total = canonical_len(value.len())?;
    let mut canonical = Vec::with_capacity(total);
    encode_bytes_object_to(value, &mut canonical)?;
    Ok(canonical)
}

/// Returns the value of a canonical bytes-role object after full framing checks.
///
/// The returned slice borrows the supplied bytes, so a caller holding
/// authenticated ownership does not copy the payload to decode it.
pub fn decode_bytes_object(canonical: &[u8]) -> ContentResult<&[u8]> {
    if canonical.len() > MAX_CANONICAL_OBJECT_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAX_CANONICAL_OBJECT_BYTES,
            actual: canonical.len(),
        });
    }
    if canonical.len() < HEADER_LEN + VALUE_LEN_BYTES {
        return Err(ContentError::UnexpectedEof);
    }
    if canonical[..OBJECT_MAGIC.len()] != OBJECT_MAGIC {
        return Err(ContentError::UnsupportedFraming);
    }
    if canonical[4] != BYTES_KIND {
        return Err(ContentError::WrongLogicalRole);
    }
    let payload_len = read_u32(canonical, 5)?;
    if payload_len > MAX_PAYLOAD_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAX_PAYLOAD_BYTES,
            actual: payload_len,
        });
    }
    let value_len = read_u32(canonical, HEADER_LEN)?;
    if value_len > MAX_OBJECT_FIELD_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAX_OBJECT_FIELD_BYTES,
            actual: value_len,
        });
    }
    let encoded_value_len = value_len
        .checked_add(VALUE_LEN_BYTES)
        .ok_or(ContentError::LengthOverflow)?;
    if encoded_value_len > payload_len {
        return Err(ContentError::UnexpectedEof);
    }
    let total = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(ContentError::LengthOverflow)?;
    if canonical.len() < total {
        return Err(ContentError::UnexpectedEof);
    }
    if encoded_value_len != payload_len || canonical.len() != total {
        return Err(ContentError::TrailingBytes);
    }
    Ok(&canonical[HEADER_LEN + VALUE_LEN_BYTES..total])
}

fn read_u32(bytes: &[u8], offset: usize) -> ContentResult<usize> {
    let end = offset.checked_add(4).ok_or(ContentError::LengthOverflow)?;
    let slice = bytes.get(offset..end).ok_or(ContentError::UnexpectedEof)?;
    let value = u32::from_be_bytes(slice.try_into().map_err(|_| ContentError::UnexpectedEof)?);
    Ok(value as usize)
}

fn write<W: Write>(writer: &mut W, bytes: &[u8]) -> ContentResult<()> {
    writer.write_all(bytes).map_err(|_| ContentError::Io)
}
