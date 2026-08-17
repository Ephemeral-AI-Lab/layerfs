use std::io::{Cursor, Read, Write};

use crate::error::{CoreError, CoreResult};
use crate::identity::{ObjectId, DIGEST_BYTES};
use crate::limits::{
    MAX_CHILD_REFERENCES, MAX_COMPONENT_BYTES, MAX_OBJECT_BYTES, MAX_OBJECT_FIELD_BYTES,
};
use crate::object::model::{DirectoryEntry, Object, ObjectKind, ObjectReference};
use crate::CanonicalName;

pub const MAGIC: [u8; 4] = *b"LFSO";
pub const HEADER_LEN: usize = 9;

pub fn encode_object(object: &Object) -> CoreResult<Vec<u8>> {
    let payload_len = payload_len(object)?;
    let total_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(CoreError::LengthOverflow)?;
    if total_len > MAX_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut output = Vec::with_capacity(total_len);
    encode_object_to(object, &mut output)?;
    Ok(output)
}

pub fn encode_object_to<W: Write>(object: &Object, writer: &mut W) -> CoreResult<()> {
    let payload_len = payload_len(object)?;
    let total_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(CoreError::LengthOverflow)?;
    if total_len > MAX_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let payload_len = u32::try_from(payload_len).map_err(|_| CoreError::LengthOverflow)?;
    write(writer, &MAGIC)?;
    write(writer, &[object.kind() as u8])?;
    write(writer, &payload_len.to_be_bytes())?;
    encode_payload_to(object, writer)
}

pub fn decode_object(bytes: &[u8]) -> CoreResult<Object> {
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    decode_object_from(Cursor::new(bytes))
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

pub fn validate_identity(bytes: &[u8], expected: ObjectId) -> CoreResult<Object> {
    if ObjectId::for_bytes(bytes) != expected {
        return Err(CoreError::IdentityMismatch);
    }
    decode_object(bytes)
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

fn payload_len(object: &Object) -> CoreResult<usize> {
    match object {
        Object::Bytes(bytes) => {
            if bytes.len() > MAX_OBJECT_FIELD_BYTES {
                return Err(CoreError::ObjectLimitExceeded);
            }
            4usize
                .checked_add(bytes.len())
                .ok_or(CoreError::LengthOverflow)
        }
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

fn write<W: Write>(writer: &mut W, bytes: &[u8]) -> CoreResult<()> {
    writer.write_all(bytes).map_err(|_| CoreError::Io)
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

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    use super::*;

    fn id(byte: u8) -> ObjectId {
        ObjectId::from_bytes(&[byte; DIGEST_BYTES]).unwrap()
    }

    fn entry(name: &str, kind: ObjectKind, byte: u8) -> DirectoryEntry {
        DirectoryEntry::new(
            CanonicalName::new(name).unwrap(),
            ObjectReference::new(kind, id(byte)),
        )
    }

    fn directory_wire(entries: &[(&[u8], ObjectKind, u8)]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_be_bytes());
        for (name, kind, byte) in entries {
            payload.extend_from_slice(&u32::try_from(name.len()).unwrap().to_be_bytes());
            payload.extend_from_slice(name);
            payload.push(*kind as u8);
            payload.extend_from_slice(&[*byte; DIGEST_BYTES]);
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.push(ObjectKind::Directory as u8);
        bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(&payload);
        bytes
    }

    struct FragmentedReader {
        bytes: Vec<u8>,
        position: usize,
        chunk_size: usize,
    }

    impl FragmentedReader {
        fn new(bytes: Vec<u8>, chunk_size: usize) -> Self {
            Self {
                bytes,
                position: 0,
                chunk_size,
            }
        }
    }

    impl Read for FragmentedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.position == self.bytes.len() {
                return Ok(0);
            }
            let length = self
                .chunk_size
                .min(buffer.len())
                .min(self.bytes.len() - self.position);
            buffer[..length].copy_from_slice(&self.bytes[self.position..self.position + length]);
            self.position += length;
            Ok(length)
        }
    }

    #[test]
    fn round_trips_and_streams_each_object_kind() {
        let objects = [
            Object::bytes(b"payload".to_vec()).unwrap(),
            Object::directory(vec![
                entry("a", ObjectKind::Bytes, 1),
                entry("b", ObjectKind::Directory, 2),
            ])
            .unwrap(),
        ];
        for object in objects {
            let bytes = encode_object(&object).unwrap();
            let mut streamed = Vec::new();
            encode_object_to(&object, &mut streamed).unwrap();
            assert_eq!(streamed, bytes);
            assert_eq!(decode_object(&bytes).unwrap(), object);
            assert_eq!(decode_object_from(Cursor::new(&bytes)).unwrap(), object);
        }
    }

    #[test]
    fn object_id_matches_hash_of_canonical_encoding() {
        let objects = [
            Object::bytes(b"payload".to_vec()).unwrap(),
            Object::directory(vec![
                entry("a", ObjectKind::Bytes, 1),
                entry("b", ObjectKind::Directory, 2),
            ])
            .unwrap(),
        ];
        for object in objects {
            let encoded = encode_object(&object).unwrap();
            assert_eq!(object.id().unwrap(), ObjectId::for_bytes(&encoded));
        }
    }

    #[test]
    fn rejects_malformed_and_noncanonical_bytes() {
        let bytes = encode_object(&Object::bytes(b"x".to_vec()).unwrap()).unwrap();
        assert_eq!(
            decode_object(&bytes[..HEADER_LEN - 1]),
            Err(CoreError::UnexpectedEof)
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(decode_object(&trailing), Err(CoreError::TrailingBytes));
        let mut marker = bytes.clone();
        marker[0] = b'X';
        assert_eq!(decode_object(&marker), Err(CoreError::Unsupported));
        let mut kind = bytes.clone();
        kind[4] = 0xff;
        assert_eq!(
            decode_object(&kind),
            Err(CoreError::InvalidObjectKind { tag: 0xff })
        );

        let entries = vec![
            entry("b", ObjectKind::Bytes, 1),
            entry("a", ObjectKind::Bytes, 2),
        ];
        assert_eq!(
            encode_object(&Object::directory(entries).unwrap()),
            Err(CoreError::NonCanonicalOrdering)
        );
    }

    #[test]
    fn rejects_truncated_payloads_and_invalid_fields() {
        let bytes = encode_object(&Object::bytes(b"payload".to_vec()).unwrap()).unwrap();
        let mut short = bytes.clone();
        short[8] -= 1;
        assert_eq!(decode_object(&short), Err(CoreError::UnexpectedEof));
        let mut long = bytes;
        long[8] += 1;
        assert_eq!(decode_object(&long), Err(CoreError::UnexpectedEof));

        let mut oversized_field = vec![];
        oversized_field.extend_from_slice(&MAGIC);
        oversized_field.push(ObjectKind::Bytes as u8);
        oversized_field.extend_from_slice(&4_u32.to_be_bytes());
        oversized_field.extend_from_slice(
            &u32::try_from(MAX_OBJECT_FIELD_BYTES + 1)
                .unwrap()
                .to_be_bytes(),
        );
        assert_eq!(
            decode_object(&oversized_field),
            Err(CoreError::ObjectLimitExceeded)
        );
    }

    #[test]
    fn rejects_declared_fields_larger_than_remaining_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.push(ObjectKind::Bytes as u8);
        bytes.extend_from_slice(&7_u32.to_be_bytes());
        bytes.extend_from_slice(&4_u32.to_be_bytes());
        bytes.extend_from_slice(b"abc");

        assert_eq!(decode_object(&bytes), Err(CoreError::UnexpectedEof));
    }

    #[test]
    fn directory_names_use_the_canonical_name_bound() {
        let oversized_name = vec![b'x'; MAX_COMPONENT_BYTES + 1];
        let bytes = directory_wire(&[(&oversized_name, ObjectKind::Bytes, 1)]);

        assert_eq!(decode_object(&bytes), Err(CoreError::ObjectLimitExceeded));
    }

    #[test]
    fn streaming_decode_handles_fragmented_and_short_reads() {
        let object = Object::bytes(b"fragmented".to_vec()).unwrap();
        let bytes = encode_object(&object).unwrap();

        let mut fragmented = FragmentedReader::new(bytes.clone(), 1);
        assert_eq!(decode_object_from(&mut fragmented).unwrap(), object);

        let mut short = FragmentedReader::new(bytes[..bytes.len() - 1].to_vec(), 2);
        assert_eq!(
            decode_object_from(&mut short),
            Err(CoreError::UnexpectedEof)
        );
    }

    #[test]
    fn rejects_manually_unsorted_directory_wire_bytes() {
        let bytes = directory_wire(&[
            (&b"b"[..], ObjectKind::Bytes, 1),
            (&b"a"[..], ObjectKind::Bytes, 2),
        ]);

        assert_eq!(decode_object(&bytes), Err(CoreError::NonCanonicalOrdering));
    }

    #[test]
    fn identity_authenticates_supplied_bytes_before_decoding() {
        let object = Object::bytes(b"identity".to_vec()).unwrap();
        let bytes = encode_object(&object).unwrap();
        let expected = object.id().unwrap();
        assert_eq!(validate_identity(&bytes, expected).unwrap(), object);

        let mut changed = bytes.clone();
        changed[HEADER_LEN + 4] = b'X';
        assert_eq!(
            validate_identity(&changed, expected),
            Err(CoreError::IdentityMismatch)
        );
        assert_eq!(
            validate_identity(&bytes, id(0)),
            Err(CoreError::IdentityMismatch)
        );
    }

    #[test]
    fn canonical_vectors_are_fixed() {
        let object = Object::bytes(b"hello".to_vec()).unwrap();
        let bytes = encode_object(&object).unwrap();
        assert_eq!(hex_bytes(&bytes), "4c46534f01000000090000000568656c6c6f");
        assert_eq!(
            object.id().unwrap().to_string(),
            "a246e43d678984a154487ee08e96f5677f0100cf59041d6708103a517e383a49"
        );

        let empty = Object::directory(Vec::new()).unwrap();
        let empty_bytes = encode_object(&empty).unwrap();
        assert_eq!(hex_bytes(&empty_bytes), "4c46534f020000000400000000");
        assert_eq!(
            empty.id().unwrap().to_string(),
            "c705a66295b38b1e1dabe72fec9c4793bde8e3bea68af1ea775a51d1cc56547a"
        );
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
