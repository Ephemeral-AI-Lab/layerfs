#![cfg(feature = "native")]
use layerfs_bridge::{adapters::native::protocol::*, contract::*};

fn directory(serial: u64) -> DirectoryMetadata {
    DirectoryMetadata {
        serial,
        mode: 0o1777,
        mtime_seconds: -1,
        mtime_nanoseconds: 123_456_789,
    }
}
fn prepared() -> PreparedChanges {
    PreparedChanges {
        workspace: [4; 32],
        branch: [0x11; 17],
        expected_head: Some([0x12; 33]),
        expected_base: [0x32; 33],
        generation: 1,
        base: [2; 32],
        scope: [3; 32],
        root_serial: 1,
        directories: vec![
            DirectoryChange {
                parent: 1,
                changes: vec![(b"d".to_vec(), Some(7))],
            },
            DirectoryChange {
                parent: 7,
                changes: vec![],
            },
        ],
        inodes: vec![],
        directory_metadata: vec![],
        new_directories: vec![directory(7)],
        new_file_serials: Vec::new(),
        new_symlink_serials: Vec::new(),
    }
}
fn request(route: u8, changes: PreparedChanges) -> Request {
    let operation = match route {
        0 => Operation::UpdatePreparedFilesystem {
            base: changes.base,
            scope: changes.scope,
            root_serial: changes.root_serial,
            directories: changes.directories,
            inodes: changes.inodes,
            new_directories: changes.new_directories,
            new_file_serials: changes.new_file_serials,
            new_symlink_serials: changes.new_symlink_serials,
            directory_metadata: changes.directory_metadata,
        },
        1 => Operation::HistoryCommand(HistoryCommand::StageChanges(changes)),
        2 => Operation::HistoryCommand(HistoryCommand::Commit(changes)),
        _ => unreachable!(),
    };
    Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: if route == 0 { 1 } else { HISTORY_PROFILE },
        deadline_ms: 5000,
        response_bytes: 0,
        operation,
    }
}
fn trailer() -> Vec<u8> {
    let mut bytes = vec![1, 0, 1];
    bytes.extend_from_slice(&7u64.to_be_bytes());
    bytes.extend_from_slice(&0o1777u32.to_be_bytes());
    bytes.extend_from_slice(&(-1i64).to_be_bytes());
    bytes.extend_from_slice(&123_456_789u32.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes
}

#[test]
fn absent_extension_keeps_legacy_bytes_and_nonempty_extension_is_explicit() {
    for route in 0..3 {
        let mut changes = prepared();
        changes.new_directories.clear();
        let legacy = request(route, changes);
        let bytes = encode_request(&legacy).unwrap();
        // Header + fixed fields + two parent records + one one-byte binding + two counts.
        assert_eq!(bytes.len(), if route == 0 { 134 } else { 259 });
        assert_eq!(decode_request(1, &bytes).unwrap(), legacy);
        let extended = request(route, prepared());
        let mut expected = bytes.clone();
        expected.extend_from_slice(&trailer());
        let actual = encode_request(&extended).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), if route == 0 { 163 } else { 288 });
        assert_eq!(decode_request(1, &actual).unwrap(), extended);
        // The old complete prefix remains a legacy message; a present trailer cannot truncate.
        for end in bytes.len() + 1..actual.len() {
            assert!(decode_request(1, &actual[..end]).is_err(), "{route}/{end}");
        }
        for (suffix, code) in [
            (vec![4], Code::Unsupported),
            (vec![1, 0, 0, 0, 0], Code::InvalidInput),
            (vec![1, 0, 129], Code::Capacity),
        ] {
            let mut invalid = bytes.clone();
            invalid.extend_from_slice(&suffix);
            assert_eq!(decode_request(1, &invalid).unwrap_err().code, code);
        }
        let mut trailing = actual;
        trailing.push(0);
        assert!(decode_request(1, &trailing).is_err());
    }
}

#[test]
fn new_directory_validation_preserves_existing_identity_and_metadata_contracts() {
    for route in 0..3 {
        for case in 0..9 {
            let mut changes = prepared();
            match case {
                0 => changes.new_directories[0].serial = 0,
                1 => changes.new_directories[0].serial = i64::MAX as u64 + 1,
                2 => changes.new_directories[0].serial = 1,
                3 => changes.new_directories[0].mode = 0o2000,
                4 => changes.new_directories[0].mtime_nanoseconds = 1_000_000_000,
                5 => {
                    changes.directories.pop();
                }
                6 => changes.new_directories.push(directory(7)),
                7 => changes.inodes.push(InodeChange {
                    serial: 7,
                    kind: 2,
                    content: [5; 32],
                    metadata: [6; 32],
                }),
                _ => {
                    changes.new_directories = vec![directory(8), directory(7)];
                    changes.directories.push(DirectoryChange {
                        parent: 8,
                        changes: vec![],
                    });
                }
            }
            assert_eq!(
                encode_request(&request(route, changes)).unwrap_err().code,
                Code::InvalidInput,
                "{route}/{case}"
            );
        }
        let mut unbound = prepared();
        unbound.directories[0].changes.clear();
        let r = request(route, unbound);
        assert_eq!(decode_request(1, &encode_request(&r).unwrap()).unwrap(), r);
        let mut changes = prepared();
        changes.inodes = (100..227)
            .map(|serial| InodeChange {
                serial,
                kind: 1,
                content: [5; 32],
                metadata: [6; 32],
            })
            .collect();
        let r = request(route, changes.clone());
        assert_eq!(decode_request(1, &encode_request(&r).unwrap()).unwrap(), r);
        changes.inodes.push(InodeChange {
            serial: 227,
            kind: 1,
            content: [5; 32],
            metadata: [6; 32],
        });
        assert_eq!(
            encode_request(&request(route, changes.clone()))
                .unwrap_err()
                .code,
            Code::Capacity
        );
        // Combined cardinality is refused before allocating the new-directory vector.
        changes.new_directories.clear();
        let mut raw = encode_request(&request(route, changes)).unwrap();
        raw.extend_from_slice(&trailer());
        assert_eq!(decode_request(1, &raw).unwrap_err().code, Code::Capacity);
        let mut maximum = prepared();
        maximum.new_directories = (2..130).map(directory).collect();
        maximum.directories = (2..130)
            .map(|parent| DirectoryChange {
                parent,
                changes: vec![],
            })
            .collect();
        let r = request(route, maximum);
        let encoded = encode_request(&r).unwrap();
        assert_eq!(decode_request(1, &encoded).unwrap(), r);
        assert_eq!(encoded.len(), if route == 0 { 4460 } else { 4585 });
    }
}

#[test]
fn complete_prepared_metadata_still_has_the_same_32kib_ceiling() {
    for route in 0..3 {
        let mut changes = prepared();
        let fixed = if route == 0 { 152 } else { 277 };
        let mut remaining = METADATA_BYTES - fixed - 10 * 128 - 4 * 128;
        changes.directories[0].changes = (0..128)
            .map(|index| {
                let extra = remaining.min(251);
                remaining -= extra;
                let mut name = format!("{index:04}").into_bytes();
                name.resize(4 + extra, b'x');
                (name, None)
            })
            .collect();
        assert_eq!(remaining, 0);
        let r = request(route, changes.clone());
        let encoded = encode_request(&r).unwrap();
        assert_eq!(encoded.len(), METADATA_BYTES);
        assert_eq!(decode_request(1, &encoded).unwrap(), r);
        changes.directories[0]
            .changes
            .iter_mut()
            .find(|(name, _)| name.len() < 255)
            .unwrap()
            .0
            .push(b'x');
        assert_eq!(
            encode_request(&request(route, changes)).unwrap_err().code,
            Code::Capacity
        );
        let mut oversized = encoded;
        oversized.push(0);
        assert_eq!(
            decode_request(1, &oversized).unwrap_err().code,
            Code::Capacity
        );
    }
}

#[test]
fn existing_directory_patches_are_disjoint_bounded_and_allow_the_root() {
    for route in 0..3 {
        let mut changes = prepared();
        changes.new_directories.clear();
        changes.directories.clear();
        changes.directory_metadata = vec![directory(1)];
        let r = request(route, changes.clone());
        let encoded = encode_request(&r).unwrap();
        assert_eq!(encoded.len(), if route == 0 { 132 } else { 257 });
        assert_eq!(decode_request(1, &encoded).unwrap(), r);
        for case in 0..7 {
            let mut invalid = changes.clone();
            match case {
                0 => invalid.directory_metadata[0].serial = 0,
                1 => invalid.directory_metadata[0].serial = i64::MAX as u64 + 1,
                2 => invalid.directory_metadata[0].mode = 0o2000,
                3 => invalid.directory_metadata[0].mtime_nanoseconds = 1_000_000_000,
                4 => invalid.directory_metadata.push(directory(1)),
                5 => invalid.directory_metadata = vec![directory(2), directory(1)],
                _ => invalid.inodes.push(InodeChange {
                    serial: 1,
                    kind: 2,
                    content: [5; 32],
                    metadata: [6; 32],
                }),
            }
            assert_eq!(
                encode_request(&request(route, invalid)).unwrap_err().code,
                Code::InvalidInput
            );
        }
        let mut overlap = prepared();
        overlap.directory_metadata = vec![directory(7)];
        assert_eq!(
            encode_request(&request(route, overlap)).unwrap_err().code,
            Code::InvalidInput
        );
        let mut combined = prepared();
        combined.directory_metadata = vec![directory(1)];
        let r = request(route, combined.clone());
        let encoded = encode_request(&r).unwrap();
        assert_eq!(encoded.len(), if route == 0 { 187 } else { 312 });
        assert_eq!(decode_request(1, &encoded).unwrap(), r);
        combined.inodes = (100..227)
            .map(|serial| InodeChange {
                serial,
                kind: 1,
                content: [5; 32],
                metadata: [6; 32],
            })
            .collect();
        assert_eq!(
            encode_request(&request(route, combined.clone()))
                .unwrap_err()
                .code,
            Code::Capacity
        );
        combined.directory_metadata.clear();
        let mut raw = encode_request(&request(route, combined)).unwrap();
        raw.truncate(raw.len() - 2);
        raw.extend_from_slice(&1u16.to_be_bytes());
        raw.extend_from_slice(&1u64.to_be_bytes());
        raw.extend_from_slice(&0o755u32.to_be_bytes());
        raw.extend_from_slice(&0i64.to_be_bytes());
        raw.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(decode_request(1, &raw).unwrap_err().code, Code::Capacity);
        changes.directory_metadata = (1..129).map(directory).collect();
        let r = request(route, changes);
        let encoded = encode_request(&r).unwrap();
        assert_eq!(encoded.len(), if route == 0 { 3180 } else { 3305 });
        assert_eq!(decode_request(1, &encoded).unwrap(), r);
    }
}
