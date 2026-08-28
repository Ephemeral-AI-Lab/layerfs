pub fn validate_identity(bytes: &[u8], expected: ObjectId) -> CoreResult<Object> {
    if ObjectId::for_bytes(bytes) != expected {
        return Err(CoreError::IdentityMismatch);
    }
    decode_object(bytes)
}

/// Authenticates canonical bytes and their outer framing without decoding the
/// role-specific payload. Callers must run the exact role decoder before using
/// the payload.
pub fn authenticate_identity(bytes: &[u8], expected: ObjectId) -> CoreResult<ObjectSummary> {
    if ObjectId::for_bytes(bytes) != expected {
        return Err(CoreError::IdentityMismatch);
    }
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    if bytes.len() < HEADER_LEN {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(CoreError::Unsupported);
    }
    let kind = ObjectKind::try_from(bytes[4])?;
    let payload_len = usize::try_from(u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]))
        .map_err(|_| CoreError::LengthOverflow)?;
    if payload_len > MAX_OBJECT_BYTES - HEADER_LEN {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let canonical_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(CoreError::LengthOverflow)?;
    if bytes.len() != canonical_len {
        return Err(if bytes.len() < canonical_len {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    Ok(ObjectSummary {
        kind,
        canonical_len: canonical_len as u64,
    })
}

pub fn validate_bytes_identity(bytes: &[u8], expected: ObjectId) -> CoreResult<&[u8]> {
    if ObjectId::for_bytes(bytes) != expected {
        return Err(CoreError::IdentityMismatch);
    }
    decode_bytes_object(bytes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectSummary {
    pub kind: ObjectKind,
    pub canonical_len: u64,
}

pub fn validate_object_from<R: Read>(reader: R) -> CoreResult<ObjectSummary> {
    let mut reader = reader;
    let mut header = [0_u8; HEADER_LEN];
    read_exact(&mut reader, &mut header)?;
    if header[..MAGIC.len()] != MAGIC {
        return Err(CoreError::Unsupported);
    }
    let kind = ObjectKind::try_from(header[4])?;
    let payload_len = usize::try_from(u32::from_be_bytes([
        header[5], header[6], header[7], header[8],
    ]))
    .map_err(|_| CoreError::LengthOverflow)?;
    let max_payload = MAX_OBJECT_BYTES
        .checked_sub(HEADER_LEN)
        .ok_or(CoreError::LengthOverflow)?;
    if payload_len > max_payload {
        return Err(CoreError::ObjectLimitExceeded);
    }

    let mut decoder = Decoder {
        reader: &mut reader,
        remaining: payload_len,
    };
    match kind {
        ObjectKind::Bytes => {
            let length =
                usize::try_from(decoder.read_u32()?).map_err(|_| CoreError::LengthOverflow)?;
            if length > MAX_OBJECT_FIELD_BYTES {
                return Err(CoreError::ObjectLimitExceeded);
            }
            decoder.discard_exact(length)?;
        }
        ObjectKind::Directory => {
            let count =
                usize::try_from(decoder.read_u32()?).map_err(|_| CoreError::LengthOverflow)?;
            if count > MAX_CHILD_REFERENCES {
                return Err(CoreError::ObjectLimitExceeded);
            }
            let mut previous: Option<CanonicalName> = None;
            for _ in 0..count {
                let name = CanonicalName::from_bytes(&decoder.read_field(MAX_COMPONENT_BYTES)?)?;
                let _child_kind = ObjectKind::try_from(decoder.read_u8()?)?;
                let _child_id = decoder.read_array::<DIGEST_BYTES>()?;
                if previous.as_ref().is_some_and(|previous| previous >= &name) {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                previous = Some(name);
            }
        }
    }
    if decoder.remaining != 0 {
        decoder.discard_remaining()?;
        return Err(CoreError::TrailingBytes);
    }
    let mut trailing = [0_u8; 1];
    match reader.read(&mut trailing) {
        Ok(0) => {
            let header_len = u64::try_from(HEADER_LEN).map_err(|_| CoreError::LengthOverflow)?;
            let payload_len = u64::try_from(payload_len).map_err(|_| CoreError::LengthOverflow)?;
            let canonical_len = header_len
                .checked_add(payload_len)
                .ok_or(CoreError::LengthOverflow)?;
            Ok(ObjectSummary {
                kind,
                canonical_len,
            })
        }
        Ok(_) => Err(CoreError::TrailingBytes),
        Err(_) => Err(CoreError::Io),
    }
}
