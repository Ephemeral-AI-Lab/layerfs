#![cfg(feature = "native")]
use layerfs_bridge::{adapters::native::protocol::*, contract::*};
use std::io::{self, Cursor, Read};
struct Fragment<'a>(&'a [u8]);
impl Read for Fragment<'_> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        let n = b.len().min(1);
        std::io::Read::read(&mut self.0, &mut b[..n])
    }
}
#[test]
fn fixed_framing_rejects_lengths_states_and_truncations() {
    let frame = Frame {
        kind: Kind::Body,
        id: 17,
        bytes: vec![1; 16384],
    };
    let bytes = frame.encode().unwrap();
    let read = Frame::read(&mut Fragment(&bytes)).unwrap();
    assert_eq!(read.bytes, frame.bytes);
    for length in 0..20 {
        assert!(Frame::read(&mut Cursor::new(&bytes[..length])).is_err());
    }
    let mut huge = bytes[..20].to_vec();
    huge[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        Frame::read(&mut Cursor::new(huge)).unwrap_err().code,
        Code::Capacity
    );
    let mut state = InputState::new(17, 1);
    state
        .accept(&Frame {
            kind: Kind::Body,
            id: 17,
            bytes: vec![1],
        })
        .unwrap();
    assert!(state
        .accept(&Frame {
            kind: Kind::EndInput,
            id: 17,
            bytes: 2u64.to_be_bytes().to_vec()
        })
        .is_err());
    let mut state = InputState::new(1, 8192);
    for _ in 0..frame_budget(8192) {
        state
            .accept(&Frame {
                kind: Kind::Body,
                id: 1,
                bytes: vec![1],
            })
            .unwrap();
    }
    assert!(state
        .accept(&Frame {
            kind: Kind::EndInput,
            id: 1,
            bytes: 8192u64.to_be_bytes().to_vec()
        })
        .is_err());
    assert!(Frame::read_optional(&mut io::empty()).unwrap().is_none());
}
#[test]
fn all_operation_metadata_roundtrips_and_caps_are_checked() {
    let operations = vec![
        Operation::SaveFile {
            base: None,
            base_length: 0,
            length: MAX_FILE,
            extents: 1,
            replacement: MAX_FILE,
        },
        Operation::ReadFile {
            root: [1; 32],
            start: 0,
            end: 4,
        },
        Operation::Inspect {
            root: [1; 32],
            query: Inspect::List {
                path: b"a/b".to_vec(),
                after: b"first".to_vec(),
                entries: 128,
                bytes: 16384,
            },
        },
        Operation::SaveFile {
            base: Some([2; 32]),
            base_length: 6,
            length: 9,
            extents: 2,
            replacement: 3,
        },
    ];
    for operation in operations {
        let r = Request {
            id: 1,
            generation: 9,
            store: 1,
            profile: 1,
            deadline_ms: 10000,
            response_bytes: if matches!(operation, Operation::SaveFile { .. }) {
                0
            } else {
                MAX_FILE
            },
            operation,
        };
        let encoded = encode_request(&r).unwrap();
        assert_eq!(decode_request(1, &encoded).unwrap(), r);
        for length in 0..encoded.len() {
            assert!(decode_request(1, &encoded[..length]).is_err());
        }
        let mut surplus = encoded;
        surplus.push(0);
        assert!(decode_request(1, &surplus).is_err());
    }
    let save = |extents: u64, replacement: u64, length: u64| Request {
        id: 1,
        generation: 0,
        store: 1,
        profile: 1,
        deadline_ms: 10000,
        response_bytes: 0,
        operation: Operation::SaveFile {
            base: Some([2; 32]),
            base_length: 5,
            length,
            extents,
            replacement,
        },
    };
    assert!(save(2, 1, 5).validate().is_ok());
    assert!(save(4_097, 1, 5).validate().is_ok());
    assert!(save(0, 1, 5).validate().is_err());
    assert!(save(1, MAX_FILE + 1, MAX_FILE).validate().is_err());
    let huge_base = Request {
        deadline_ms: 10000,
        operation: Operation::SaveFile {
            base: Some([2; 32]),
            base_length: MAX_FILE + 1,
            length: 5,
            extents: 1,
            replacement: 1,
        },
        ..save(1, 1, 5)
    };
    assert!(huge_base.validate().is_err());
}

#[test]
fn final_file_stream_charges_extent_records_separately_from_file_length() {
    let unbounded = Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10_000,
        response_bytes: 0,
        operation: Operation::SaveFile {
            base: Some([3; 32]),
            base_length: 1 << 40,
            length: MAX_FILE,
            extents: 4_097,
            replacement: 1 << 32,
        },
    };
    assert!(unbounded.validate().is_err());
    assert_eq!(
        unbounded.operation.input_length().unwrap(),
        4_097 * 24 + (1 << 32)
    );
    let bounded = Request {
        operation: Operation::SaveFile {
            base: Some([3; 32]),
            base_length: MAX_FILE,
            length: MAX_FILE,
            extents: 2,
            replacement: 1 << 20,
        },
        ..unbounded
    };
    assert!(bounded.validate().is_ok());
    assert_eq!(bounded.operation.input_length().unwrap(), 48 + (1 << 20));
}

#[test]
fn terminal_types_and_explicit_continuation_roundtrip() {
    let values = [
        Response::List {
            entries: vec![(b"a".to_vec(), 2)],
            continuation: Some(b"a".to_vec()),
        },
        Response::List {
            entries: vec![],
            continuation: None,
        },
    ];
    for value in values {
        assert_eq!(
            decode_response(&encode_response(&value).unwrap()).unwrap(),
            value
        );
    }
}

#[test]
fn remote_budget_only_shortens_the_declared_duration() {
    let request = Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 1000,
        response_bytes: 0,
        operation: Operation::SaveFile {
            base: None,
            base_length: 0,
            length: 0,
            extents: 0,
            replacement: 0,
        },
    };
    let bytes = encode_request_with_budget(&request, 37).unwrap();
    let remote = decode_request(request.id, &bytes).unwrap();
    assert_eq!(remote.deadline_ms, 37);
    assert_eq!(remote.operation, request.operation);
    assert!(encode_request_with_budget(&request, 0).is_err());
    assert!(encode_request_with_budget(&request, 1001).is_err());
}

#[test]
fn declared_large_input_has_a_size_consistent_frame_and_time_budget() {
    let mut request = Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: MAX_OPERATION_MS,
        response_bytes: 0,
        operation: Operation::SaveFile {
            base: None,
            base_length: 0,
            length: MAX_FILE,
            extents: 1,
            replacement: MAX_FILE,
        },
    };
    request.validate().unwrap();
    assert!(frame_budget(MAX_FILE) > MAX_FILE / FRAME_BYTES as u64);
    request.operation = Operation::SaveFile {
        base: None,
        base_length: 0,
        length: MAX_FILE + 1,
        extents: 1,
        replacement: MAX_FILE + 1,
    };
    assert!(request.validate().is_err());
    request.operation = Operation::SaveFile {
        base: None,
        base_length: 0,
        length: 0,
        extents: 0,
        replacement: 0,
    };
    request.deadline_ms = MAX_OPERATION_MS + 1;
    assert_eq!(request.validate().unwrap_err().code, Code::InvalidInput);
}
