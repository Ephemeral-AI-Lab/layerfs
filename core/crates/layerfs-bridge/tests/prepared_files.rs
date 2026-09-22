#![cfg(feature = "native")]
//! Fresh regular-file declarations are an explicit version-two prepared trailer.
use layerfs_bridge::{adapters::native::protocol::*, contract::*};

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
        directories: vec![DirectoryChange {
            parent: 1,
            changes: vec![(b"fresh".to_vec(), Some(7))],
        }],
        inodes: vec![InodeChange {
            serial: 7,
            kind: 1,
            content: [5; 32],
            metadata: [6; 32],
        }],
        new_directories: vec![],
        directory_metadata: vec![],
        new_file_serials: vec![7],
    }
}
fn request(route: u8, p: PreparedChanges) -> Request {
    let operation = match route {
        0 => Operation::UpdatePreparedFilesystem {
            base: p.base,
            scope: p.scope,
            root_serial: p.root_serial,
            directories: p.directories,
            inodes: p.inodes,
            new_directories: p.new_directories,
            directory_metadata: p.directory_metadata,
            new_file_serials: p.new_file_serials,
        },
        1 => Operation::HistoryCommand(HistoryCommand::StageChanges(p)),
        2 => Operation::HistoryCommand(HistoryCommand::Commit(p)),
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
fn directory(serial: u64) -> DirectoryMetadata {
    DirectoryMetadata {
        serial,
        mode: 0o755,
        mtime_seconds: -2,
        mtime_nanoseconds: 17,
    }
}
fn directory_bytes(d: DirectoryMetadata) -> Vec<u8> {
    [
        &d.serial.to_be_bytes()[..],
        &d.mode.to_be_bytes(),
        &d.mtime_seconds.to_be_bytes(),
        &d.mtime_nanoseconds.to_be_bytes(),
    ]
    .concat()
}

#[test]
fn fresh_file_trailer_preserves_omitted_and_v1_bytes_on_every_prepared_route() {
    for route in 0..3 {
        let mut p = prepared();
        p.new_file_serials.clear();
        let legacy = encode_request(&request(route, p.clone())).unwrap();
        assert_eq!(
            decode_request(1, &legacy).unwrap(),
            request(route, p.clone())
        );
        let mut v2 = legacy.clone();
        v2.extend_from_slice(&[2, 0, 0, 0, 0, 0, 1]);
        v2.extend_from_slice(&7u64.to_be_bytes());
        assert_eq!(encode_request(&request(route, prepared())).unwrap(), v2);
        assert_eq!(decode_request(1, &v2).unwrap(), request(route, prepared()));
        for end in legacy.len() + 1..v2.len() {
            assert!(decode_request(1, &v2[..end]).is_err());
        }
        p.new_directories.push(directory(8));
        p.directories.push(DirectoryChange {
            parent: 8,
            changes: vec![],
        });
        p.directory_metadata.push(directory(1));
        let mut without = p.clone();
        without.new_directories.clear();
        without.directory_metadata.clear();
        let prefix = encode_request(&request(route, without)).unwrap();
        let mut v1 = prefix.clone();
        v1.extend_from_slice(&[1, 0, 1]);
        v1.extend_from_slice(&directory_bytes(directory(8)));
        v1.extend_from_slice(&1u16.to_be_bytes());
        v1.extend_from_slice(&directory_bytes(directory(1)));
        assert_eq!(encode_request(&request(route, p.clone())).unwrap(), v1);
        assert_eq!(decode_request(1, &v1).unwrap(), request(route, p.clone()));
        p.new_file_serials.push(7);
        let mut combined = prefix.clone();
        combined.extend_from_slice(&[2, 0, 1]);
        combined.extend_from_slice(&directory_bytes(directory(8)));
        combined.extend_from_slice(&1u16.to_be_bytes());
        combined.extend_from_slice(&directory_bytes(directory(1)));
        combined.extend_from_slice(&1u16.to_be_bytes());
        combined.extend_from_slice(&7u64.to_be_bytes());
        assert_eq!(combined.len() - prefix.len(), 7 + 24 * 2 + 8);
        assert_eq!(
            encode_request(&request(route, p.clone())).unwrap(),
            combined
        );
        assert_eq!(decode_request(1, &combined).unwrap(), request(route, p));
        let mut empty = legacy.clone();
        empty.extend_from_slice(&[2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            decode_request(1, &empty).unwrap_err().code,
            Code::InvalidInput
        );
        let mut huge_count = legacy.clone();
        huge_count.extend_from_slice(&[2, 0, 0, 0, 0, 0, 2]);
        assert_eq!(
            decode_request(1, &huge_count).unwrap_err().code,
            Code::Capacity
        );
        let mut trailing = combined;
        trailing.push(0);
        assert!(decode_request(1, &trailing).is_err());
    }
}

#[test]
fn fresh_file_validation_is_bounded_regular_only_and_disjoint() {
    for route in 0..3 {
        for case in 0..11 {
            let mut p = prepared();
            let mut expected = Code::InvalidInput;
            match case {
                0 => p.new_file_serials[0] = 0,
                1 => p.new_file_serials[0] = i64::MAX as u64 + 1,
                2 => p.new_file_serials[0] = 1,
                3 => p.new_file_serials[0] = 8,
                4 => p.inodes[0].kind = 2,
                5 => p.inodes[0].kind = 3,
                6 => {
                    p.new_file_serials.push(7);
                    expected = Code::Capacity;
                }
                7 => {
                    p.inodes.push(InodeChange {
                        serial: 8,
                        ..p.inodes[0]
                    });
                    p.new_file_serials = vec![7, 7];
                }
                8 => {
                    p.inodes.push(InodeChange {
                        serial: 8,
                        ..p.inodes[0]
                    });
                    p.new_file_serials = vec![8, 7];
                }
                9 => {
                    p.new_directories.push(directory(7));
                    p.directories.push(DirectoryChange {
                        parent: 7,
                        changes: vec![],
                    });
                }
                _ => p.directory_metadata.push(directory(7)),
            }
            assert_eq!(
                encode_request(&request(route, p)).unwrap_err().code,
                expected,
                "{route}/{case}"
            );
        }
        let mut terminal = prepared();
        terminal.new_file_serials[0] = i64::MAX as u64;
        terminal.inodes[0].serial = i64::MAX as u64;
        terminal.directories[0].changes[0].1 = Some(i64::MAX as u64);
        let terminal = request(route, terminal);
        assert_eq!(
            decode_request(1, &encode_request(&terminal).unwrap()).unwrap(),
            terminal
        );
        let mut p = prepared();
        p.inodes = (2..130)
            .map(|serial| InodeChange {
                serial,
                kind: 1,
                content: [5; 32],
                metadata: [6; 32],
            })
            .collect();
        p.new_file_serials = (2..130).collect();
        p.directories[0].changes = (2..130)
            .map(|serial| (format!("f{serial:03}").into_bytes(), Some(serial)))
            .collect();
        let encoded = encode_request(&request(route, p.clone())).unwrap();
        assert_eq!(
            decode_request(1, &encoded).unwrap(),
            request(route, p.clone())
        );
        // F is a subset annotation: I128/F128 is legal, not a 256-row frontier.
        p.directory_metadata.push(directory(1));
        assert_eq!(
            encode_request(&request(route, p)).unwrap_err().code,
            Code::Capacity
        );
    }
}

#[test]
fn fresh_file_v2_still_obeys_the_exact_32kib_request_ceiling() {
    for route in 0..3 {
        let mut p = prepared();
        p.directories[0].changes = (0..128)
            .map(|i| (format!("{i:04}").into_bytes(), Some(7)))
            .collect();
        let initial = encode_request(&request(route, p.clone())).unwrap().len();
        let mut remaining = METADATA_BYTES - initial;
        for (name, _) in &mut p.directories[0].changes {
            let extra = remaining.min(251);
            name.resize(name.len() + extra, b'x');
            remaining -= extra;
        }
        assert_eq!(remaining, 0);
        let bytes = encode_request(&request(route, p.clone())).unwrap();
        assert_eq!(bytes.len(), METADATA_BYTES);
        assert_eq!(
            decode_request(1, &bytes).unwrap(),
            request(route, p.clone())
        );
        p.directories[0]
            .changes
            .iter_mut()
            .find(|(n, _)| n.len() < 255)
            .unwrap()
            .0
            .push(b'x');
        assert_eq!(
            encode_request(&request(route, p)).unwrap_err().code,
            Code::Capacity
        );
        let mut over = bytes;
        over.push(0);
        assert_eq!(decode_request(1, &over).unwrap_err().code, Code::Capacity);
    }
}
