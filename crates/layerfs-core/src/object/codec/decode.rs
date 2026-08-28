pub fn decode_object(bytes: &[u8]) -> CoreResult<Object> {
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    decode_object_from(Cursor::new(bytes))
}

pub fn decode_bytes_object(bytes: &[u8]) -> CoreResult<&[u8]> {
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    if bytes.len() < HEADER_LEN {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(CoreError::Unsupported);
    }
    if ObjectKind::try_from(bytes[4])? != ObjectKind::Bytes {
        decode_object(bytes)?;
        return Err(CoreError::WrongLogicalRole);
    }
    let payload_len = usize::try_from(u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]))
        .map_err(|_| CoreError::LengthOverflow)?;
    let max_payload = MAX_OBJECT_BYTES
        .checked_sub(HEADER_LEN)
        .ok_or(CoreError::LengthOverflow)?;
    if payload_len > max_payload {
        return Err(CoreError::ObjectLimitExceeded);
    }
    if payload_len < 4 || bytes.len() < HEADER_LEN + 4 {
        return Err(CoreError::UnexpectedEof);
    }
    let value_len = usize::try_from(u32::from_be_bytes([
        bytes[HEADER_LEN],
        bytes[HEADER_LEN + 1],
        bytes[HEADER_LEN + 2],
        bytes[HEADER_LEN + 3],
    ]))
    .map_err(|_| CoreError::LengthOverflow)?;
    if value_len > MAX_OBJECT_FIELD_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let encoded_value_len = 4usize
        .checked_add(value_len)
        .ok_or(CoreError::LengthOverflow)?;
    if encoded_value_len > payload_len {
        return Err(CoreError::UnexpectedEof);
    }
    let canonical_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(CoreError::LengthOverflow)?;
    if bytes.len() < canonical_len {
        return Err(CoreError::UnexpectedEof);
    }
    if encoded_value_len != payload_len || bytes.len() != canonical_len {
        return Err(CoreError::TrailingBytes);
    }
    Ok(&bytes[HEADER_LEN + 4..canonical_len])
}

pub fn decode_object_from<R: Read>(reader: R) -> CoreResult<Object> {
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
    let object = decode_payload(kind, &mut decoder)?;
    if decoder.remaining != 0 {
        decoder.discard_remaining()?;
        return Err(CoreError::TrailingBytes);
    }
    let mut trailing = [0_u8; 1];
    match reader.read(&mut trailing) {
        Ok(0) => Ok(object),
        Ok(_) => Err(CoreError::TrailingBytes),
        Err(_) => Err(CoreError::Io),
    }
}

fn decode_payload<R: Read>(kind: ObjectKind, decoder: &mut Decoder<'_, R>) -> CoreResult<Object> {
    match kind {
        ObjectKind::Bytes => Object::bytes(decoder.read_field(MAX_OBJECT_FIELD_BYTES)?),
        ObjectKind::Directory => {
            let count =
                usize::try_from(decoder.read_u32()?).map_err(|_| CoreError::LengthOverflow)?;
            if count > MAX_CHILD_REFERENCES {
                return Err(CoreError::ObjectLimitExceeded);
            }
            let mut entries = Vec::with_capacity(count);
            let mut previous: Option<CanonicalName> = None;
            for _ in 0..count {
                let name = CanonicalName::from_bytes(&decoder.read_field(MAX_COMPONENT_BYTES)?)?;
                let child_kind = ObjectKind::try_from(decoder.read_u8()?)?;
                let child_id = ObjectId::from_bytes(&decoder.read_array::<DIGEST_BYTES>()?)?;
                if previous.as_ref().is_some_and(|previous| previous >= &name) {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                previous = Some(name.clone());
                entries.push(DirectoryEntry::new(
                    name,
                    ObjectReference::new(child_kind, child_id),
                ));
            }
            Object::directory(entries)
        }
    }
}

fn read_exact<R: Read>(reader: &mut R, bytes: &mut [u8]) -> CoreResult<()> {
    reader.read_exact(bytes).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            CoreError::UnexpectedEof
        } else {
            CoreError::Io
        }
    })
}

struct Decoder<'a, R> {
    reader: &'a mut R,
    remaining: usize,
}

impl<R: Read> Decoder<'_, R> {
    fn read_exact(&mut self, bytes: &mut [u8]) -> CoreResult<()> {
        if bytes.len() > self.remaining {
            return Err(CoreError::UnexpectedEof);
        }
        read_exact(self.reader, bytes)?;
        self.remaining -= bytes.len();
        Ok(())
    }

    fn read_array<const N: usize>(&mut self) -> CoreResult<[u8; N]> {
        let mut bytes = [0_u8; N];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> CoreResult<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u32(&mut self) -> CoreResult<u32> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }

    fn read_field(&mut self, max_length: usize) -> CoreResult<Vec<u8>> {
        let length = usize::try_from(self.read_u32()?).map_err(|_| CoreError::LengthOverflow)?;
        if length > max_length {
            return Err(CoreError::ObjectLimitExceeded);
        }
        if length > self.remaining {
            return Err(CoreError::UnexpectedEof);
        }
        let mut bytes = vec![0_u8; length];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn discard_remaining(&mut self) -> CoreResult<()> {
        let mut buffer = [0_u8; 4096];
        while self.remaining != 0 {
            let length = self.remaining.min(buffer.len());
            self.read_exact(&mut buffer[..length])?;
        }
        Ok(())
    }

    fn discard_exact(&mut self, mut length: usize) -> CoreResult<()> {
        let mut buffer = [0_u8; 4096];
        while length != 0 {
            let take = length.min(buffer.len());
            self.read_exact(&mut buffer[..take])?;
            length -= take;
        }
        Ok(())
    }
}
