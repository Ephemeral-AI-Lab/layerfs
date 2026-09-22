#![cfg(feature = "native")]
//! Version-three fresh symlink declarations preserve all older prepared encodings.
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
            changes: vec![(b"file".to_vec(), Some(7)), (b"link".to_vec(), Some(8))],
        }],
        inodes: vec![
            InodeChange {
                serial: 7,
                kind: 1,
                content: [5; 32],
                metadata: [6; 32],
            },
            InodeChange {
                serial: 8,
                kind: 3,
                content: [7; 32],
                metadata: [8; 32],
            },
        ],
        new_directories: vec![],
        directory_metadata: vec![],
        new_file_serials: vec![7],
        new_symlink_serials: vec![8],
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
            new_symlink_serials: p.new_symlink_serials,
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
fn v3_is_present_only_with_symlinks_and_preserves_legacy_v1_v2_bytes() {
    for route in 0..3 {
        let mut p = prepared();
        p.new_file_serials.clear();
        p.new_symlink_serials.clear();
        let legacy = encode_request(&request(route, p.clone())).unwrap();
        assert_eq!(
            decode_request(1, &legacy).unwrap(),
            request(route, p.clone())
        );
        p.directory_metadata.push(directory(1));
        let mut v1 = legacy.clone();
        v1.extend([1, 0, 0, 0, 1]);
        v1.extend(directory_bytes(directory(1)));
        assert_eq!(encode_request(&request(route, p.clone())).unwrap(), v1);
        assert_eq!(decode_request(1, &v1).unwrap(), request(route, p.clone()));
        p.directory_metadata.clear();
        p.new_file_serials.push(7);
        let mut v2 = legacy.clone();
        v2.extend([2, 0, 0, 0, 0, 0, 1]);
        v2.extend(7u64.to_be_bytes());
        assert_eq!(encode_request(&request(route, p.clone())).unwrap(), v2);
        assert_eq!(decode_request(1, &v2).unwrap(), request(route, p));
        for files in [false, true] {
            let mut p = prepared();
            if !files {
                p.new_file_serials.clear();
            }
            let mut v3 = legacy.clone();
            v3.extend([3, 0, 0, 0, 0]);
            v3.extend((usize::from(files) as u16).to_be_bytes());
            if files {
                v3.extend(7u64.to_be_bytes());
            }
            v3.extend(1u16.to_be_bytes());
            v3.extend(8u64.to_be_bytes());
            assert_eq!(v3.len() - legacy.len(), 9 + 8 * (1 + usize::from(files)));
            assert_eq!(encode_request(&request(route, p.clone())).unwrap(), v3);
            assert_eq!(decode_request(1, &v3).unwrap(), request(route, p));
            for n in legacy.len() + 1..v3.len() {
                assert!(decode_request(1, &v3[..n]).is_err());
            }
            let mut trailing = v3;
            trailing.push(0);
            assert!(decode_request(1, &trailing).is_err());
        }
        let mut p = prepared();
        p.new_directories.push(directory(9));
        p.directory_metadata.push(directory(1));
        p.directories.push(DirectoryChange {
            parent: 9,
            changes: vec![],
        });
        let mut prefix = p.clone();
        prefix.new_file_serials.clear();
        prefix.new_symlink_serials.clear();
        prefix.new_directories.clear();
        prefix.directory_metadata.clear();
        let mut expected = encode_request(&request(route, prefix)).unwrap();
        let length = expected.len();
        expected.extend([3, 0, 1]);
        expected.extend(directory_bytes(directory(9)));
        expected.extend(1u16.to_be_bytes());
        expected.extend(directory_bytes(directory(1)));
        expected.extend(1u16.to_be_bytes());
        expected.extend(7u64.to_be_bytes());
        expected.extend(1u16.to_be_bytes());
        expected.extend(8u64.to_be_bytes());
        assert_eq!(expected.len() - length, 9 + 24 * 2 + 8 * 2);
        assert_eq!(
            encode_request(&request(route, p.clone())).unwrap(),
            expected
        );
        assert_eq!(decode_request(1, &expected).unwrap(), request(route, p));
    }
}

#[test]
fn symlink_annotations_are_sorted_kind3_subsets_and_counted_once() {
    for route in 0..3 {
        for case in 0..12 {
            let mut p = prepared();
            let mut code = Code::InvalidInput;
            match case {
                0 => p.new_symlink_serials[0] = 0,
                1 => p.new_symlink_serials[0] = i64::MAX as u64 + 1,
                2 => p.new_symlink_serials[0] = 1,
                3 => p.new_symlink_serials[0] = 9,
                4 => p.inodes[1].kind = 1,
                5 => p.inodes[1].kind = 2,
                6 => {
                    p.new_symlink_serials.push(8);
                    code = Code::Capacity;
                }
                7 => {
                    p.new_file_serials.clear();
                    p.inodes[0].kind = 3;
                    p.new_symlink_serials = vec![8, 8];
                }
                8 => {
                    p.new_file_serials.clear();
                    p.inodes[0].kind = 3;
                    p.new_symlink_serials = vec![8, 7];
                }
                9 => p.new_symlink_serials[0] = 7,
                10 => {
                    p.new_directories.push(directory(8));
                    p.directories.push(DirectoryChange {
                        parent: 8,
                        changes: vec![],
                    });
                }
                _ => p.directory_metadata.push(directory(8)),
            }
            assert_eq!(
                encode_request(&request(route, p)).unwrap_err().code,
                code,
                "{route}/{case}"
            );
        }
        let mut p = prepared();
        p.inodes[1].serial = i64::MAX as u64;
        p.new_symlink_serials[0] = i64::MAX as u64;
        p.directories[0].changes[1].1 = Some(i64::MAX as u64);
        let r = request(route, p);
        assert_eq!(decode_request(1, &encode_request(&r).unwrap()).unwrap(), r);
        let mut p = prepared();
        p.inodes = (2..130)
            .map(|serial| InodeChange {
                serial,
                kind: if serial < 66 { 1 } else { 3 },
                content: [5; 32],
                metadata: [6; 32],
            })
            .collect();
        p.new_file_serials = (2..66).collect();
        p.new_symlink_serials = (66..130).collect();
        p.directories[0].changes = (2..130)
            .map(|s| (format!("n{s:03}").into_bytes(), Some(s)))
            .collect();
        let r = request(route, p.clone());
        assert_eq!(decode_request(1, &encode_request(&r).unwrap()).unwrap(), r);
        p.directory_metadata.push(directory(1));
        assert_eq!(
            encode_request(&request(route, p)).unwrap_err().code,
            Code::Capacity
        );
    }
}

#[test]
fn v3_refuses_zero_or_excess_symlink_count_before_payload_allocation() {
    for route in 0..3 {
        let mut p = prepared();
        p.new_file_serials.clear();
        p.new_symlink_serials.clear();
        let prefix = encode_request(&request(route, p)).unwrap();
        let mut empty = prefix.clone();
        empty.extend([3, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            decode_request(1, &empty).unwrap_err().code,
            Code::InvalidInput
        );
        let mut over = prefix.clone();
        over.extend([3, 0, 0, 0, 0, 0, 1]);
        over.extend(7u64.to_be_bytes());
        over.extend(2u16.to_be_bytes());
        // I=2 and F=1 leave only one S entry. No S payload is provided.
        assert_eq!(decode_request(1, &over).unwrap_err().code, Code::Capacity);
        let mut enormous = prefix.clone();
        enormous.extend([3, 0, 0, 0, 0, 0, 0, 255, 255]);
        assert_eq!(
            decode_request(1, &enormous).unwrap_err().code,
            Code::Capacity
        );
        let mut missing = prefix.clone();
        missing.extend([3, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(
            decode_request(1, &missing).unwrap_err().code,
            Code::Capacity
        );
        let mut unknown = prefix;
        unknown.push(4);
        assert_eq!(
            decode_request(1, &unknown).unwrap_err().code,
            Code::Unsupported
        );
        let mut p = prepared();
        p.directories[0].changes = (0..128)
            .map(|i| {
                (
                    format!("{i:04}").into_bytes(),
                    Some(if i == 0 { 8 } else { 7 }),
                )
            })
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
            .find(|(name, _)| name.len() < 255)
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
