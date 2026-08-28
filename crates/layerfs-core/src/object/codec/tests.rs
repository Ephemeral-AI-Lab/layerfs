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
    fn borrowed_bytes_encoding_matches_owned_canonical_identity() {
        for value in [&b""[..], &b"payload"[..]] {
            let borrowed = encode_bytes_object(value).unwrap();
            let mut streamed = Vec::with_capacity(borrowed.len());
            encode_bytes_object_to(value, &mut streamed).unwrap();
            let owned = encode_object(&Object::bytes(value.to_vec()).unwrap()).unwrap();
            assert_eq!(borrowed, owned);
            assert_eq!(streamed, borrowed);
            assert_eq!(ObjectId::for_bytes(&borrowed), ObjectId::for_bytes(&owned));
            assert_eq!(decode_bytes_object(&borrowed).unwrap(), value);
        }
    }

    #[test]
    fn shallow_authentication_defers_payload_rules_to_the_exact_role_decoder() {
        let mut bytes = MAGIC.to_vec();
        bytes.push(ObjectKind::Bytes as u8);
        bytes.extend_from_slice(&4_u32.to_be_bytes());
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        let id = ObjectId::for_bytes(&bytes);
        assert_eq!(
            authenticate_identity(&bytes, id).unwrap(),
            ObjectSummary {
                kind: ObjectKind::Bytes,
                canonical_len: bytes.len() as u64,
            }
        );
        assert_eq!(decode_bytes_object(&bytes), Err(CoreError::UnexpectedEof));
        assert_eq!(
            authenticate_identity(&bytes, ObjectId::for_bytes(b"other")),
            Err(CoreError::IdentityMismatch)
        );
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
        assert_eq!(
            validate_bytes_identity(&bytes, expected).unwrap(),
            b"identity"
        );

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
        assert_eq!(
            validate_bytes_identity(&changed, expected),
            Err(CoreError::IdentityMismatch)
        );
        assert_eq!(
            decode_bytes_object(&bytes[..bytes.len() - 1]),
            Err(CoreError::UnexpectedEof)
        );
        let directory = encode_object(&Object::directory(Vec::new()).unwrap()).unwrap();
        assert_eq!(
            decode_bytes_object(&directory),
            Err(CoreError::WrongLogicalRole)
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
