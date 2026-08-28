pub fn encode_object(object: &Object) -> CoreResult<Vec<u8>> {
    let payload_len = payload_len(object)?;
    let total_len = checked_total_len(payload_len)?;
    let mut output = Vec::with_capacity(total_len);
    encode_object_to(object, &mut output)?;
    Ok(output)
}

pub fn encode_bytes_object(value: &[u8]) -> CoreResult<Vec<u8>> {
    let payload_len = bytes_payload_len(value)?;
    let total_len = checked_total_len(payload_len)?;
    let mut output = Vec::with_capacity(total_len);
    encode_bytes_object_to(value, &mut output)?;
    Ok(output)
}

pub fn encode_bytes_object_to<W: Write>(value: &[u8], writer: &mut W) -> CoreResult<()> {
    let payload_len = bytes_payload_len(value)?;
    checked_total_len(payload_len)?;
    encode_header_to(ObjectKind::Bytes, payload_len, writer)?;
    let length = u32::try_from(value.len()).map_err(|_| CoreError::LengthOverflow)?;
    write(writer, &length.to_be_bytes())?;
    write(writer, value)
}

pub fn encode_object_to<W: Write>(object: &Object, writer: &mut W) -> CoreResult<()> {
    let payload_len = payload_len(object)?;
    checked_total_len(payload_len)?;
    encode_header_to(object.kind(), payload_len, writer)?;
    encode_payload_to(object, writer)
}

fn encode_header_to<W: Write>(
    kind: ObjectKind,
    payload_len: usize,
    writer: &mut W,
) -> CoreResult<()> {
    let payload_len = u32::try_from(payload_len).map_err(|_| CoreError::LengthOverflow)?;
    write(writer, &MAGIC)?;
    write(writer, &[kind as u8])?;
    write(writer, &payload_len.to_be_bytes())
}

fn payload_len(object: &Object) -> CoreResult<usize> {
    match object {
        Object::Bytes(bytes) => bytes_payload_len(bytes),
        Object::Directory(entries) => {
            if entries.len() > MAX_CHILD_REFERENCES {
                return Err(CoreError::ObjectLimitExceeded);
            }
            let mut total = 4usize;
            let mut previous: Option<&CanonicalName> = None;
            for entry in entries {
                if let Some(previous) = previous {
                    if previous >= entry.name() {
                        return Err(CoreError::NonCanonicalOrdering);
                    }
                }
                previous = Some(entry.name());
                total = total
                    .checked_add(4)
                    .and_then(|value| value.checked_add(entry.name().as_bytes().len()))
                    .and_then(|value| value.checked_add(1 + DIGEST_BYTES))
                    .ok_or(CoreError::LengthOverflow)?;
            }
            Ok(total)
        }
    }
}

fn bytes_payload_len(bytes: &[u8]) -> CoreResult<usize> {
    if bytes.len() > MAX_OBJECT_FIELD_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    4usize
        .checked_add(bytes.len())
        .ok_or(CoreError::LengthOverflow)
}

fn checked_total_len(payload_len: usize) -> CoreResult<usize> {
    let total_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(CoreError::LengthOverflow)?;
    if total_len > MAX_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(total_len)
}

fn encode_payload_to<W: Write>(object: &Object, writer: &mut W) -> CoreResult<()> {
    match object {
        Object::Bytes(bytes) => {
            let length = u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
            write(writer, &length.to_be_bytes())?;
            write(writer, bytes)?;
        }
        Object::Directory(entries) => {
            let count = u32::try_from(entries.len()).map_err(|_| CoreError::LengthOverflow)?;
            write(writer, &count.to_be_bytes())?;
            for entry in entries {
                let name = entry.name().as_bytes();
                let length = u32::try_from(name.len()).map_err(|_| CoreError::LengthOverflow)?;
                write(writer, &length.to_be_bytes())?;
                write(writer, name)?;
                let reference = entry.reference();
                write(writer, &[reference.kind() as u8])?;
                write(writer, reference.id().as_bytes())?;
            }
        }
    }
    Ok(())
}

fn write<W: Write>(writer: &mut W, bytes: &[u8]) -> CoreResult<()> {
    writer.write_all(bytes).map_err(|_| CoreError::Io)
}
