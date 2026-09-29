#![cfg(feature = "native")]
//! Authenticated internal version 2 has distinct opcodes and exactly typed replies.
use layerfs_bridge::{
    adapters::native::protocol::{
        decode_request, decode_response, encode_request, encode_response,
    },
    contract::{
        permission_bit, Code, Operation, Request, Response, FILE_SAVE_CAPABILITIES_OPCODE,
        SAVE_FILE_OPCODE, SAVE_FILE_V2_OPCODE, SAVE_FILE_V2_VERSION,
    },
};

fn request(operation: Operation) -> Request {
    Request {
        id: 73,
        generation: 0,
        store: 1,
        profile: 1,
        deadline_ms: 10_000,
        response_bytes: 0,
        operation,
    }
}

#[test]
fn authenticated_capability_and_v1_v2_metadata_are_distinct() {
    let probe = request(Operation::FileSaveCapabilities);
    let probe_bytes = encode_request(&probe).unwrap();
    assert_eq!(probe_bytes[26], FILE_SAVE_CAPABILITIES_OPCODE);
    assert_eq!(decode_request(probe.id, &probe_bytes).unwrap(), probe);
    assert!(probe.operation.read_only());
    assert_eq!(
        permission_bit(FILE_SAVE_CAPABILITIES_OPCODE),
        permission_bit(SAVE_FILE_OPCODE)
    );
    let response = Response::FileSaveCapabilities {
        version: SAVE_FILE_V2_VERSION,
    };
    assert_eq!(
        decode_response(&encode_response(&response).unwrap()).unwrap(),
        response
    );
    let base = Some([19; 32]);
    let v1 = request(Operation::SaveFile {
        base,
        base_length: 4,
        length: 7,
        extents: 2,
        replacement: 3,
    });
    let v2 = request(Operation::SaveFileV2 {
        base,
        base_length: 4,
        length: 7,
        extents: 2,
        replacement: 3,
    });
    assert_eq!(v1.operation.input_length().unwrap(), 51);
    assert_eq!(v2.operation.input_length().unwrap(), 52);
    for (original, opcode) in [(v1, SAVE_FILE_OPCODE), (v2, SAVE_FILE_V2_OPCODE)] {
        assert!(original.operation.content_mutation());
        assert_eq!(permission_bit(opcode), permission_bit(SAVE_FILE_OPCODE));
        let encoded = encode_request(&original).unwrap();
        assert_eq!(encoded[26], opcode);
        assert_eq!(decode_request(original.id, &encoded).unwrap(), original);
        assert_eq!(
            decode_request(original.id, &encoded[..encoded.len() - 1])
                .unwrap_err()
                .code,
            Code::InvalidInput
        );
    }
}
