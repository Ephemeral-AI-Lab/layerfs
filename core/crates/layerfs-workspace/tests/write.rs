//! Handle writes exercise the same native Workspace, backing and Commit route.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        io::Write,
        sync::{atomic::AtomicBool, Barrier},
        time::{Duration, Instant},
    };

    fn check(id: &str) {
        println!("WRITE_CHECK {id} PASS");
    }
    fn options(access: FileAccess, append: bool) -> FileOpenOptions {
        FileOpenOptions {
            access,
            append,
            truncate: false,
        }
    }
    fn open(f: &Fixture, access: FileAccess, append: bool) -> (NodeAttributes, HandleId) {
        let data = f.lookup(b"data.bin");
        let h = f
            .workspace
            .open_file(
                data.serial,
                options(access, append),
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        (data, h)
    }
    fn write(f: &Fixture, h: HandleId, offset: u64, bytes: &[u8]) -> MutationReceipt {
        let input = f.own(bytes);
        let result = f
            .workspace
            .write_file(h, offset, &input, deadline())
            .unwrap();
        assert_eq!(result.accepted_bytes, bytes.len() as u64);
        result
    }
    fn saved(f: &Fixture, report: &CommitReport) -> (u64, Root, u64, i64, u32) {
        let root = match &report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            CommitOutcomeWire::UpToDate { root, .. } => *root,
        };
        let data = attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(attr(f.native.attributes(root, b"alias")), data);
        data
    }
    fn observe(f: &Fixture) {
        println!(
            "COMMIT_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn close(f: &Fixture, data: NodeAttributes, handles: &[HandleId]) {
        for h in handles {
            f.workspace.release(*h).unwrap();
        }
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
    }
    fn payload_bytes(input: &OwnedPayload) -> Vec<u8> {
        let mut reader = input.reader(0..input.len()).unwrap();
        let mut result = vec![0; input.len() as usize];
        assert_eq!(
            reader
                .read(&mut result, deadline(), &AtomicBool::new(false))
                .unwrap(),
            result.len()
        );
        result
    }

    #[test]
    #[ignore = "requires write_route.py native fixture"]
    fn write_positional() {
        let f = Fixture::new(Gate::None);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        f.workspace.set_len(data.serial, 16, deadline()).unwrap();
        let mut expected: Vec<u8> = (0..16).collect();
        write(&f, h, 3, b"HELLO");
        expected[3..8].copy_from_slice(b"HELLO");
        assert_eq!(f.read(h, 0, 32), expected);
        write(&f, h, 14, b"WXYZ");
        expected.truncate(14);
        expected.extend_from_slice(b"WXYZ");
        assert_eq!(f.read(h, 0, 32), expected);
        write(&f, h, 25, b"abc");
        expected.resize(25, 0);
        expected.extend_from_slice(b"abc");
        assert_eq!(f.read(h, 0, 32), expected);
        assert_eq!(f.workspace.handle_attributes(h).unwrap().size, 28);
        let live = f.workspace.getattr(data.serial).unwrap();
        let first = f.workspace.commit(deadline()).unwrap();
        let g = saved(&f, &first);
        assert_eq!((g.3, g.4), (live.mtime_seconds, live.mtime_nanoseconds));
        assert_eq!(f.native.bytes(g.1, 0, expected.len()), expected);
        let begin = f.native.observations.lock().unwrap().operations.len();
        write(&f, h, 4, b"!");
        expected[4] = b'!';
        let second = f.workspace.commit(deadline()).unwrap();
        let d = saved(&f, &second);
        assert_eq!(f.native.bytes(d.1, 0, expected.len()), expected);
        let observed = f.native.observations.lock().unwrap();
        let edits: Vec<_> = observed.operations[begin..]
            .iter()
            .filter_map(|op| {
                if let Operation::SaveFile {
                    base: Some(root),
                    base_length,
                    replacement,
                    ..
                } = op
                {
                    Some((*root, *base_length, *replacement))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(edits, vec![(g.1, 28, 1)]);
        assert!(!observed.operations.iter().any(|op| matches!(
            op,
            Operation::HistoryCommand(HistoryCommand::StageChanges(_))
        )));
        drop(observed);
        observe(&f);
        close(&f, data, &[h]);
        check("positional-overwrite-tail-zero-gap-and-incremental-Commit");
    }

    #[test]
    #[ignore = "requires write_route.py native append fixture"]
    fn write_append() {
        let f = Fixture::new(Gate::None);
        let (data, h) = open(&f, FileAccess::ReadWrite, true);
        f.workspace.set_len(data.serial, 0, deadline()).unwrap();
        write(&f, h, u64::MAX, b"A");
        write(&f, h, 0, b"BC");
        f.workspace.set_len(data.serial, 5, deadline()).unwrap();
        write(&f, h, 1, b"D");
        assert_eq!(f.read(h, 0, 10), b"ABC\0\0D");
        let (_, second) = open(&f, FileAccess::ReadWrite, true);
        let x = f.own(b"X");
        let y = f.own(b"Y");
        let start = Barrier::new(2);
        let (a, b) = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                start.wait();
                f.workspace.write_file(h, 0, &x, deadline())
            });
            let b = scope.spawn(|| {
                start.wait();
                f.workspace.write_file(second, 0, &y, deadline())
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        assert!(a.is_ok() || b.is_ok());
        for (result, handle, input) in [(a, h, &x), (b, second, &y)] {
            match result {
                Ok(r) => assert_eq!(r.accepted_bytes, 1),
                Err(WorkspaceError::Busy) => {
                    assert_eq!(
                        f.workspace
                            .write_file(handle, 0, input, deadline())
                            .unwrap()
                            .accepted_bytes,
                        1
                    );
                }
                Err(e) => panic!("unexpected append failure {e:?}"),
            }
        }
        let bytes = f.read(h, 0, 16);
        assert_eq!(&bytes[..6], b"ABC\0\0D");
        assert!(bytes[6..] == *b"XY" || bytes[6..] == *b"YX");
        let result = f.workspace.commit(deadline()).unwrap();
        let content = saved(&f, &result);
        assert_eq!(f.native.bytes(content.1, 0, bytes.len()), bytes);
        drop(x);
        drop(y);
        observe(&f);
        close(&f, data, &[h, second]);
        check("append-selects-live-EOF-and-concurrent-successes-never-overwrite");
    }

    #[test]
    #[ignore = "requires write_route.py native handle rights fixture"]
    fn write_zero_rights() {
        let f = Fixture::new(Gate::None);
        let (data, h) = open(&f, FileAccess::WriteOnly, false);
        let (_, ro) = open(&f, FileAccess::ReadOnly, false);
        let directory = f
            .workspace
            .opendir(f.workspace.root().serial, ReferenceScope::Local)
            .unwrap();
        let empty = f.own(b"");
        let input = f.own(b"x");
        let before = f.workspace.status().unwrap();
        let metadata = f.workspace.metadata_status().unwrap();
        let backing = f.workspace.backing_status().unwrap();
        let calls = f.native.observations.lock().unwrap().operations.len();
        let result = f
            .workspace
            .write_file(h, data.size + 10, &empty, deadline())
            .unwrap();
        assert_eq!(
            (result.generation, result.revision, result.accepted_bytes),
            (before.generation, before.revision, 0)
        );
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        assert_eq!(f.workspace.metadata_status().unwrap().roots, metadata.roots);
        assert_eq!(
            f.workspace.backing_status().unwrap().allocated_bytes,
            backing.allocated_bytes
        );
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            calls
        );
        for handle in [ro, directory, 0, u64::MAX] {
            for token in [&empty, &input] {
                assert!(f
                    .workspace
                    .write_file(handle, 0, token, deadline())
                    .is_err());
            }
        }
        assert!(matches!(
            f.workspace.write_file(h, u64::MAX, &input, deadline()),
            Err(WorkspaceError::Capacity)
        ));
        assert!(matches!(
            f.workspace.write_file(h, MAX_FILE, &input, deadline()),
            Err(WorkspaceError::Capacity)
        ));
        assert!(matches!(
            f.workspace
                .write_file(h, 0, &input, Instant::now() - Duration::from_secs(1)),
            Err(WorkspaceError::Deadline)
        ));
        let peer = f
            .host
            .attach(
                Fixture::options("peer", 32, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        let foreign = peer.own_payload(1, &mut &b"f"[..], deadline()).unwrap();
        assert!(matches!(
            f.workspace.write_file(h, 0, &foreign, deadline()),
            Err(WorkspaceError::InvalidInput)
        ));
        drop(foreign);
        peer.close_clean().unwrap();
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        let ro_workspace = f
            .host
            .attach(
                Fixture::options("ro", 33, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        assert!(matches!(
            ro_workspace.write_file(h, 0, &empty, deadline()),
            Err(WorkspaceError::ReadOnly)
        ));
        ro_workspace.close_clean().unwrap();
        f.workspace.release(h).unwrap();
        assert!(matches!(
            f.workspace.write_file(h, 0, &empty, deadline()),
            Err(WorkspaceError::BadHandle)
        ));
        drop(empty);
        drop(input);
        f.workspace.releasedir(directory).unwrap();
        close(&f, data, &[ro]);
        check("zero-write-validates-rights-identity-offset-deadline-without-publication");
    }

    #[test]
    #[ignore = "requires write_route.py past-8MiB combined input fixture"]
    fn write_envelope() {
        let f = Fixture::new(Gate::None);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        f.workspace.set_len(data.serial, 0, deadline()).unwrap();
        // The replay ceiling is gone: a replacement larger than the retired
        // 8 MiB bound now lowers and streams as one edit whose replacement
        // total is the declared stream total, and the exact final bytes -
        // one whole zero gap plus the payload - publish through Commit.
        let end = 9 * 1024 * 1024u64;
        write(&f, h, end - 1, b"x");
        write(&f, h, end - 2, b"y");
        let before = f.workspace.status().unwrap();
        assert_eq!(before.dirty_inodes, 1);
        for offset in (0..end - 2).step_by(MAX_READ_BYTES) {
            let length = ((end - 2 - offset) as usize).min(MAX_READ_BYTES);
            assert_eq!(f.read(h, offset, length), vec![0; length]);
        }
        assert_eq!(f.read(h, end - 2, 2), b"yx");
        let report = f.workspace.commit(deadline()).unwrap();
        let content = saved(&f, &report);
        assert_eq!(content.2, end);
        assert_eq!(f.native.bytes(content.1, end - 3, 3), [0, b'y', b'x']);
        let observed = f.native.observations.lock().unwrap();
        let totals: Vec<u64> = observed
            .operations
            .iter()
            .filter_map(|o| {
                if let Operation::SaveFile {
                    base: Some(_),
                    replacement,
                    ..
                } = o
                {
                    Some(*replacement)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(totals, vec![end]);
        drop(observed);
        observe(&f);
        close(&f, data, &[h]);
        check("Zero-gap-and-local-input-stream-past-the-retired-8MiB-replay-ceiling");
    }

    #[test]
    #[ignore = "requires write_route.py multi-edit frontier selection"]
    fn write_frontier() {
        let f = Fixture::new(Gate::None);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        let runs = 1_024u64;
        for i in 0..runs {
            write(&f, h, i * 2, b"x");
        }
        for i in 0..64u64 {
            write(&f, h, i * 2, b"y");
        }
        assert!(f.workspace.backing_status().unwrap().payloads >= runs as usize);
        let report = f
            .workspace
            .commit(Instant::now() + Duration::from_secs(25))
            .unwrap();
        let content = saved(&f, &report);
        let span = (runs * 2 + 2) as usize;
        let bytes = f.native.bytes(content.1, 0, span);
        for (i, byte) in bytes.iter().enumerate() {
            assert_eq!(
                *byte,
                if i < runs as usize * 2 && i % 2 == 0 {
                    if i < 128 {
                        b'y'
                    } else {
                        b'x'
                    }
                } else {
                    (i % 251) as u8
                }
            );
        }
        let observed = f.native.observations.lock().unwrap();
        let declared: Vec<(u64, u64)> = observed
            .operations
            .iter()
            .filter_map(|o| {
                if let Operation::SaveFile {
                    base: Some(_),
                    extents,
                    replacement,
                    ..
                } = o
                {
                    Some((*extents, *replacement))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(declared.len(), 1);
        assert!(declared[0].0 >= runs);
        assert_eq!(declared[0].1, runs);
        drop(observed);
        observe(&f);
        close(&f, data, &[h]);
        check("final-file-save-streams-1024-separated-runs");
    }
    #[test]
    #[ignore = "requires native candidate-quota refusal"]
    fn write_quota() {
        let f = Fixture::with_quota(Gate::None, 1024 * 1024);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        let input = f.own(b"owned");
        let before = f.workspace.status().unwrap();
        assert!(f.workspace.write_file(h, 0, &input, deadline()).is_err());
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        assert_eq!(payload_bytes(&input), b"owned");
        assert_eq!(f.read(h, 0, 4), [0, 1, 2, 3]);
        drop(input);
        observe(&f);
        close(&f, data, &[h]);
        check("candidate-quota-refusal-keeps-visible-file-and-borrowed-payload");
    }
    struct RestoreLimit;
    impl Drop for RestoreLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires native private-file allocation failure"]
    fn write_metadata_failure() {
        let f = Fixture::new(Gate::None);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        let input = f.own(b"owned");
        let before = f.workspace.status().unwrap();
        let restore = RestoreLimit;
        file_limit("2048");
        let result = f.workspace.write_file(h, 0, &input, deadline());
        drop(restore);
        assert!(matches!(result, Err(WorkspaceError::Backing(_))));
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        assert_eq!(payload_bytes(&input), b"owned");
        assert_eq!(f.read(h, 0, 4), [0, 1, 2, 3]);
        assert!(f.workspace.metadata_status().unwrap().admission_stopped);
        println!("COMMIT_FAILURE {result:?}");
        observe(&f);
        check("native-write-metadata-failure-retains-version-input-and-quarantine");
    }

    fn stale() -> (Fixture, NodeAttributes, HandleId) {
        let f = Fixture::new(Gate::AttributesAfterCommit);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        f.workspace.set_len(data.serial, 8, deadline()).unwrap();
        f.workspace.commit(deadline()).unwrap();
        (f, data, h)
    }
    #[test]
    #[ignore = "requires real canonical refresh held while handle is released"]
    fn write_released() {
        let (f, data, h) = stale();
        let input = f.own(b"X");
        let before = f.workspace.status().unwrap();
        std::thread::scope(|scope| {
            let pending = scope.spawn(|| f.workspace.write_file(h, 0, &input, deadline()));
            f.native.wait_attributes();
            f.workspace.release(h).unwrap();
            f.workspace
                .forget(data.serial, u64::MAX, ReferenceScope::Local);
            f.native.release_attributes();
            assert!(matches!(
                pending.join().unwrap(),
                Err(WorkspaceError::BadHandle)
            ));
        });
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        assert_eq!(payload_bytes(&input), b"X");
        drop(input);
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("release-before-write-publication-refuses-without-consuming-borrowed-input");
    }
    #[test]
    #[ignore = "requires expired deadline during real canonical refresh"]
    fn write_deadline() {
        let (f, data, h) = stale();
        let input = f.own(b"X");
        let before = f.workspace.status().unwrap();
        let attr = f.workspace.getattr(data.serial).unwrap();
        let end = Instant::now() + Duration::from_millis(200);
        std::thread::scope(|scope| {
            let pending = scope.spawn(|| f.workspace.write_file(h, 0, &input, end));
            f.native.wait_attributes();
            while Instant::now() <= end {
                std::thread::sleep(Duration::from_millis(10));
            }
            f.native.release_attributes();
            assert!(pending.join().unwrap().is_err());
        });
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), attr);
        assert_eq!(payload_bytes(&input), b"X");
        drop(input);
        observe(&f);
        close(&f, data, &[h]);
        check("expired-write-refresh-publishes-no-bytes-length-or-timestamp");
    }
    #[test]
    #[ignore = "requires a real pending truncating-open handle"]
    fn write_pending() {
        let (f, data, h) = stale();
        let input = f.own(b"X");
        let empty = f.own(b"");
        std::thread::scope(|scope| {
            let pending = scope.spawn(|| {
                f.workspace.open_file(
                    data.serial,
                    FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        append: false,
                        truncate: true,
                    },
                    ReferenceScope::Local,
                    deadline(),
                )
            });
            f.native.wait_attributes();
            for token in [&input, &empty] {
                assert!(matches!(
                    f.workspace.write_file(h + 1, 0, token, deadline()),
                    Err(WorkspaceError::BadHandle)
                ));
            }
            f.native.release_attributes();
            let ready = pending.join().unwrap().unwrap();
            assert_eq!(ready, h + 1);
            f.workspace.release(ready).unwrap();
        });
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, 0);
        f.workspace.commit(deadline()).unwrap();
        drop(input);
        drop(empty);
        observe(&f);
        close(&f, data, &[h]);
        check("pending-open-handles-refuse-nonempty-and-zero-writes");
    }

    #[test]
    #[ignore = "requires frozen delivery and live handle writes"]
    fn write_successor() {
        let f = Fixture::new(Gate::Delivery);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        f.workspace.set_len(data.serial, 32, deadline()).unwrap();
        write(&f, h, 10, b"GGGG");
        let old = f.workspace.read(h, 10, 4, deadline()).unwrap();
        let first = std::thread::scope(|scope| {
            let saving = scope.spawn(|| f.workspace.commit(deadline()));
            f.native.wait_entered();
            write(&f, h, 40, b"LIVE");
            assert_eq!(f.read(h, 32, 12), [vec![0; 8], b"LIVE".to_vec()].concat());
            assert_eq!(old.as_ref(), b"GGGG");
            f.native.release();
            saving.join().unwrap().unwrap()
        });
        let g = saved(&f, &first);
        assert_eq!(g.2, 32);
        assert_eq!(f.native.bytes(g.1, 10, 4), b"GGGG");
        assert_eq!(f.workspace.handle_attributes(h).unwrap().size, 44);
        drop(old);
        let begin = f.native.observations.lock().unwrap().operations.len();
        let second = f.workspace.commit(deadline()).unwrap();
        let d = saved(&f, &second);
        assert_eq!(d.2, 44);
        assert_eq!(
            f.native.bytes(d.1, 32, 12),
            [vec![0; 8], b"LIVE".to_vec()].concat()
        );
        let observed = f.native.observations.lock().unwrap();
        let edits: Vec<_> = observed.operations[begin..]
            .iter()
            .filter_map(|op| {
                if let Operation::SaveFile {
                    base: Some(root),
                    base_length,
                    replacement,
                    ..
                } = op
                {
                    Some((*root, *base_length, *replacement))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(edits, vec![(g.1, 32, 12)]);
        drop(observed);
        observe(&f);
        close(&f, data, &[h]);
        check("frozen-G-and-live-positional-gap-use-exact-successive-Commit-coordinates");
    }
    #[test]
    #[ignore = "requires observed real C2 save with live write/append progress"]
    fn write_native_save() {
        let f = Fixture::new(Gate::NativeSave);
        let (data, h) = open(&f, FileAccess::ReadWrite, false);
        let (_, append) = open(&f, FileAccess::ReadWrite, true);
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for byte in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *byte = random as u8;
        }
        write(&f, h, 0, &bytes);
        f.workspace
            .set_len(data.serial, bytes.len() as u64, deadline())
            .unwrap();
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        write(&f, h, bytes.len() as u64 + 16, b"TAIL");
        write(&f, append, 0, b"END");
        let tail = [vec![0; 16], b"TAILEND".to_vec()].concat();
        assert_eq!(f.read(h, bytes.len() as u64, 23), tail);
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let first = saving.join().unwrap().unwrap();
        let g = saved(&f, &first);
        assert_eq!(g.2, bytes.len() as u64);
        assert_eq!(
            f.native.bytes(g.1, 0, MAX_READ_BYTES),
            bytes[..MAX_READ_BYTES]
        );
        let second = f.workspace.commit(deadline()).unwrap();
        let d = saved(&f, &second);
        assert_eq!(d.2, bytes.len() as u64 + 23);
        assert_eq!(f.native.bytes(d.1, bytes.len() as u64, 23), tail);
        observe(&f);
        close(&f, data, &[h, append]);
        check("positional-write-and-append-progress-during-actual-service-save");
    }
}
