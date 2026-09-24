//! The merged content-and-metadata save keeps its own identity and authority.
//!
//! One dirty file must be publishable in one crossing, so the request carries
//! the edit and the portable fields it stamps together. It is still a content
//! mutation under the content grant, and its declared input is the edit's
//! replacement bytes and nothing more.
#![cfg(feature = "native")]
use layerfs_bridge::{
    adapters::native::protocol::*,
    contract::{
        permission_bit, Edit, Inspect, Operation, Request, CONSTRUCT_PORTABLE_METADATA_OPCODE,
        EDIT_FILE_WITH_METADATA_OPCODE, UPDATE_PORTABLE_METADATA_OPCODE,
    },
};

fn request() -> Request {
    Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: 1,
        deadline_ms: 30_000,
        response_bytes: 0,
        operation: Operation::EditFileWithMetadata {
            root: [3; 32],
            base_length: 1_048_576,
            edits: vec![Edit {
                start: 522_240,
                end: 526_336,
                replacement: 4096,
            }],
            kind: 1,
            mode: 0o100644,
            mtime_seconds: 1_700_000_000,
            mtime_nanoseconds: 42,
        },
    }
}

#[test]
fn merged_save_roundtrips_with_its_own_opcode_and_grant() {
    let r = request();
    assert_eq!(r.operation.opcode(), EDIT_FILE_WITH_METADATA_OPCODE);
    assert_eq!(r.operation.label(), "EditFileWithMetadata");
    assert!(!r.operation.read_only());
    assert!(r.operation.mutation());
    assert!(r.operation.content_mutation());
    assert!(!r.operation.metadata_mutation());
    // The edit grant, exactly as a lone EditFile uses.
    assert_eq!(
        permission_bit(r.operation.opcode()),
        permission_bit(
            Operation::EditFile {
                root: [3; 32],
                base_length: 1_048_576,
                edits: Vec::new(),
            }
            .opcode()
        )
    );
    // It declares only the replacement bytes it will upload.
    assert_eq!(r.operation.input_length().unwrap(), 4096);
    assert_eq!(
        decode_request(1, &encode_request(&r).unwrap()).unwrap(),
        r,
        "the merged request must survive its own encoding"
    );
}

#[test]
fn merged_save_is_distinct_from_both_operations_it_replaces() {
    let merged = EDIT_FILE_WITH_METADATA_OPCODE;
    assert_ne!(merged, 4);
    assert_ne!(merged, UPDATE_PORTABLE_METADATA_OPCODE);
    assert_ne!(merged, CONSTRUCT_PORTABLE_METADATA_OPCODE);
    let opcodes: Vec<u8> = [
        Operation::ReadFile {
            root: [0; 32],
            start: 0,
            end: 1,
        },
        Operation::Inspect {
            root: [0; 32],
            query: Inspect::File,
        },
        Operation::ConstructFile { length: 1 },
        Operation::ConstructSymlink { target: vec![1] },
        Operation::EditFile {
            root: [0; 32],
            base_length: 1,
            edits: Vec::new(),
        },
        Operation::EditFileWithMetadata {
            root: [0; 32],
            base_length: 1,
            edits: Vec::new(),
            kind: 1,
            mode: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        },
        Operation::UpdatePortableMetadata {
            base: [0; 32],
            kind: 1,
            mode: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        },
        Operation::ConstructPortableMetadata {
            kind: 1,
            mode: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        },
    ]
    .iter()
    .map(|operation| operation.opcode())
    .collect();
    let mut unique = opcodes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), opcodes.len(), "every opcode stays distinct");
}
