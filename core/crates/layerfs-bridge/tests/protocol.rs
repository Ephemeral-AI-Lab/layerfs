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
        Operation::ConstructFile { length: MAX_FILE },
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
        Operation::EditFile {
            root: [2; 32],
            base_length: 6,
            edits: vec![Edit {
                start: 0,
                end: 1,
                replacement: 3,
            }],
        },
        Operation::UpdatePreparedFilesystem {
            base: [1; 32],
            scope: [2; 32],
            root_serial: 1,
            directories: vec![DirectoryChange {
                parent: 1,
                changes: vec![(b"f".to_vec(), Some(2))],
            }],
            inodes: vec![InodeChange {
                serial: 2,
                kind: 1,
                content: [3; 32],
                metadata: [4; 32],
            }],
        },
    ];
    for operation in operations {
        let r = Request {
            id: 1,
            generation: 9,
            store: 1,
            profile: 1,
            deadline_ms: 10000,
            response_bytes: MAX_FILE,
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
    let r = Request {
        id: 1,
        generation: 0,
        store: 1,
        profile: 1,
        deadline_ms: 10000,
        response_bytes: MAX_FILE,
        operation: Operation::EditFile {
            root: [0; 32],
            base_length: 5,
            edits: vec![
                Edit {
                    start: 0,
                    end: 0,
                    replacement: u64::MAX,
                },
                Edit {
                    start: 0,
                    end: 0,
                    replacement: 1,
                },
            ],
        },
    };
    assert!(r.validate().is_err());
}

#[test]
fn terminal_types_and_explicit_continuation_roundtrip() {
    let values = [
        Response::FilesystemSaved {
            root: [1; 32],
            inserted: 4,
            reused: 2,
        },
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
        operation: Operation::ConstructFile { length: 0 },
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
        response_bytes: MAX_FILE,
        operation: Operation::ConstructFile { length: MAX_FILE },
    };
    request.validate().unwrap();
    assert!(frame_budget(MAX_FILE) > MAX_FILE / FRAME_BYTES as u64);
    request.operation = Operation::ConstructFile {
        length: MAX_FILE + 1,
    };
    assert_eq!(request.validate().unwrap_err().code, Code::Capacity);
    request.operation = Operation::ConstructFile { length: 0 };
    request.deadline_ms = MAX_OPERATION_MS + 1;
    assert_eq!(request.validate().unwrap_err().code, Code::InvalidInput);
}
