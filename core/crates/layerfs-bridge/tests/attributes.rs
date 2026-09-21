#![cfg(feature = "native")]
use layerfs_bridge::{adapters::native::protocol::*, contract::*};

fn attributes(kind: u8, references: u64, size: u64, mode: u32) -> Response {
    Response::Attributes {
        serial: 9,
        kind,
        references,
        content: [3; 32],
        metadata: [4; 32],
        mode,
        mtime: -2,
        nanoseconds: 750_000_000,
        size,
    }
}

#[test]
fn complete_attributes_roundtrip_and_reject_invalid_fields() {
    let request = Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 1000,
        response_bytes: 0,
        operation: Operation::Inspect {
            root: [2; 32],
            query: Inspect::Attributes {
                path: b"file".to_vec(),
            },
        },
    };
    let encoded = encode_request(&request).unwrap();
    assert_eq!(encoded[59], 4, "new Inspect subtag preserves 0 through 3");
    assert_eq!(decode_request(1, &encoded).unwrap(), request);
    for response in [
        attributes(1, 3, MAX_FILE, 0o755),
        attributes(2, 0, 0, 0o1777),
        attributes(2, 1, 0, 0o755),
        attributes(3, 1, 4096, 0o777),
    ] {
        let encoded = encode_response(&response).unwrap();
        assert_eq!(encoded[0], 9, "old result tags retain their values");
        assert_eq!(decode_response(&encoded).unwrap(), response);
    }
    for response in [
        attributes(0, 1, 1, 0o644),
        attributes(1, 0, 1, 0o644),
        attributes(1, 1, MAX_FILE + 1, 0o644),
        attributes(1, 1, 1, 0o4755),
        attributes(2, 2, 0, 0o755),
        attributes(2, 1, 1, 0o755),
        attributes(3, 2, 1, 0o777),
        attributes(3, 1, 4097, 0o777),
        attributes(3, 1, 1, 0o755),
    ] {
        assert_eq!(
            encode_response(&response).unwrap_err().code,
            Code::InvalidInput
        );
    }
    let valid = encode_response(&attributes(1, 1, 8, 0o644)).unwrap();
    for (start, bytes) in [
        (1, 0u64.to_be_bytes().to_vec()),
        (1, u64::MAX.to_be_bytes().to_vec()),
        (9, vec![0]),
        (94, 1_000_000_000u32.to_be_bytes().to_vec()),
    ] {
        let mut invalid = valid.clone();
        invalid[start..start + bytes.len()].copy_from_slice(&bytes);
        assert_eq!(
            decode_response(&invalid).unwrap_err().code,
            Code::InvalidInput
        );
    }
    assert!(attributes(2, 0, 0, 0o755)
        .validate_attributes(Some(true))
        .is_ok());
    assert!(attributes(2, 0, 0, 0o755)
        .validate_attributes(Some(false))
        .is_err());
    assert!(attributes(1, 1, 1, 0o644)
        .validate_attributes(Some(true))
        .is_err());
}
