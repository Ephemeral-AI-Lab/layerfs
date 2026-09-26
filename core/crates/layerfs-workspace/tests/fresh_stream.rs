//! Fresh full construction and the distinct captured-generation replay bound.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, File, OpenOptions},
        io::{self, Read, Write},
        os::unix::fs::{FileExt, OpenOptionsExt},
        path::PathBuf,
        time::Instant,
    };

    const INPUT_LENGTH: u64 = 18_259_144;
    const INPUT_SHA256: &str = "264d3092d69de80f5acdb71c930efec8db5bd9627f41659ed3416566b9ae34b4";
    const BLOCK: usize = 128 * 1024;
    const REPLAY: u64 = 8 * 1024 * 1024;

    fn check(id: &str) {
        println!("FRESH_STREAM_CHECK {id} PASS");
    }
    fn fixture(gate: Gate) -> Fixture {
        let native = Native::new_fresh(gate);
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()),
                max_count: 3,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            native.delivery(),
        )
        .unwrap();
        let workspace = host
            .attach(
                Fixture::options("stage", 31, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        Fixture {
            host,
            workspace,
            native,
        }
    }
    fn operations(f: &Fixture) -> Vec<Operation> {
        f.native.observations.lock().unwrap().operations.clone()
    }
    fn publications(f: &Fixture) -> usize {
        operations(f)
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    Operation::HistoryCommand(
                        HistoryCommand::Commit(_) | HistoryCommand::CommitStaged { .. }
                    )
                )
            })
            .count()
    }
    fn snapshot(f: &Fixture) -> BranchSnapshotWire {
        let Response::History(value) = f.branch() else {
            panic!("history")
        };
        let HistoryResult::BranchSnapshot(value) = *value else {
            panic!("branch")
        };
        value
    }
    fn committed(f: &Fixture, report: CommitReport) -> Root {
        assert!(f.workspace.status().unwrap().submission.is_none());
        let CommitOutcomeWire::Committed(value) = report.outcome else {
            panic!("Commit")
        };
        assert_eq!(snapshot(f).effective_root, value.root);
        value.root
    }
    fn prepared(ops: &[Operation]) -> &PreparedChanges {
        let rows: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 1);
        rows[0]
    }
    fn saved(f: &Fixture, root: Root, name: &[u8], attrs: NodeAttributes) -> (Root, Root) {
        let Response::Attributes {
            serial,
            kind,
            references,
            size,
            mode,
            mtime,
            nanoseconds,
            content,
            metadata,
        } = f.native.attributes(root, name)
        else {
            panic!("attributes")
        };
        assert_eq!(
            (serial, kind, references, size, mode, mtime, nanoseconds),
            (
                attrs.serial,
                1,
                1,
                attrs.size,
                attrs.mode,
                attrs.mtime_seconds,
                attrs.mtime_nanoseconds
            )
        );
        (content, metadata)
    }
    fn missing(f: &Fixture, root: Root, name: &[u8]) {
        assert_eq!(
            f.native
                .request(
                    Operation::Inspect {
                        root,
                        query: Inspect::Attributes {
                            path: name.to_vec()
                        }
                    },
                    0,
                    &mut io::sink()
                )
                .unwrap_err()
                .code,
            Code::PathNotFound
        );
    }
    #[derive(Default)]
    struct DigestSink {
        digest: Sha256,
        bytes: u64,
    }
    impl Write for DigestSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.digest.update(bytes);
            self.bytes += bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn canonical_digest(f: &Fixture, root: Root, length: u64, expected: &str) {
        let mut sink = DigestSink::default();
        assert_eq!(
            f.native
                .request(
                    Operation::ReadFile {
                        root,
                        start: 0,
                        end: length
                    },
                    length,
                    &mut sink
                )
                .unwrap(),
            Response::Read { length }
        );
        assert_eq!(sink.bytes, length);
        assert_eq!(format!("{:x}", sink.digest.finalize()), expected);
    }
    fn quiescent(f: &Fixture, closed_handles: bool) {
        let end = deadline();
        loop {
            let s = f.workspace.status().unwrap();
            if s.projection_replies == 0 && (!closed_handles || s.projection_handles == 0) {
                break;
            }
            assert!(Instant::now() < end, "{s:?}");
            std::thread::yield_now();
        }
    }
    fn writable_mount(f: &Fixture) -> layerfs_fuse::MountHandle {
        let mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
        let row = table
            .lines()
            .find(|line| {
                line.split(' ').nth(4) == Some(f.workspace.mount_path().to_str().unwrap())
                    && (line.contains(" - fuse layerfs ")
                        || line.contains(" - fuse.layerfs layerfs "))
            })
            .expect("owned FUSE mount");
        let options = row.split(' ').nth(5).unwrap();
        assert!(options.split(',').any(|s| s == "rw"), "{row}");
        assert!(!options.split(',').any(|s| s == "ro"), "{row}");
        println!("MKDIR_RESOURCE mount_profile=writable actual_mount_options={options}");
        mount
    }
    fn close(f: &Fixture, serial: u64) {
        f.workspace.forget(serial, 1, ReferenceScope::Local);
        assert_eq!(f.workspace.getattr(serial), Err(WorkspaceError::NotFound));
        println!(
            "MKDIR_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
        f.workspace.close_clean().unwrap();
        check("native-clean-close");
    }
    fn one_construct(ops: &[Operation], length: u64) {
        assert_eq!(
            ops.iter()
                .filter_map(|op| match op {
                    Operation::SaveFile {
                        base: None, length, ..
                    } => Some(*length),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![length]
        );
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op, Operation::ConstructPortableMetadata { kind: 1, .. }))
                .count(),
            1
        );
        assert!(!ops
            .iter()
            .any(|op| matches!(op, Operation::SaveFile { base: Some(_), .. })));
    }
    fn one_edit(ops: &[Operation], content: Root, metadata: Root, length: u64, replacement: u64) {
        let rows: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                Operation::SaveFile {
                    base: Some(root),
                    base_length,
                    replacement,
                    ..
                } => Some((root, base_length, replacement)),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!((*rows[0].0, *rows[0].1), (content, length));
        assert_eq!(*rows[0].2, replacement);
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op,
            Operation::UpdatePortableMetadata { base, .. } if *base == metadata))
                .count(),
            1
        );
        assert!(!ops.iter().any(|op| matches!(
            op,
            Operation::SaveFile { base: None, .. } | Operation::ConstructPortableMetadata { .. }
        )));
        assert!(prepared(ops).new_file_serials.is_empty());
    }

    #[test]
    #[ignore = "requires the pinned preinstalled DSH input and actual writable FUSE"]
    fn fresh_stream_mounted_dsh() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        let mut input = File::open("/runner/dsh-largest-input").unwrap();
        let meta = input.metadata().unwrap();
        assert!(meta.is_file());
        assert_eq!(meta.len(), INPUT_LENGTH);
        let mut output = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o644)
            .open(f.workspace.mount_path().join("dsh-largest"))
            .unwrap();
        let mut buffer = [0u8; BLOCK];
        let (mut original, mut edited) = (Sha256::new(), Sha256::new());
        let (mut offset, mut blocks) = (0u64, 0usize);
        while offset < INPUT_LENGTH {
            let size = (INPUT_LENGTH - offset).min(BLOCK as u64) as usize;
            input.read_exact(&mut buffer[..size]).unwrap();
            original.update(&buffer[..size]);
            if offset == 0 {
                assert_eq!(&buffer[..4], b"\x7fELF");
                edited.update(b"EDIT");
                edited.update(&buffer[4..size]);
            } else {
                edited.update(&buffer[..size]);
            }
            output.write_all(&buffer[..size]).unwrap();
            offset += size as u64;
            blocks += 1;
        }
        assert_eq!(input.read(&mut buffer[..1]).unwrap(), 0);
        let uploaded = format!("{:x}", original.finalize());
        assert_eq!(uploaded, INPUT_SHA256);
        let edited_hash = format!("{:x}", edited.finalize());
        assert_eq!(blocks, 140);
        drop(input);
        quiescent(&f, false);
        let a = f.lookup(b"dsh-largest");
        assert_eq!(a.size, INPUT_LENGTH);
        assert_eq!(publications(&f), 0);
        assert_eq!(snapshot(&f), before);
        missing(&f, before.effective_root, b"dsh-largest");
        let start = operations(&f).len();
        let first = committed(&f, f.workspace.commit(deadline()).unwrap());
        let ops = &operations(&f)[start..];
        one_construct(ops, INPUT_LENGTH);
        assert_eq!(prepared(ops).new_file_serials, vec![a.serial]);
        assert_eq!(publications(&f), 1);
        let (content, metadata) = saved(&f, first, b"dsh-largest", a);
        canonical_digest(&f, content, INPUT_LENGTH, INPUT_SHA256);
        output.write_all_at(b"EDIT", 0).unwrap();
        quiescent(&f, false);
        let live = f.workspace.getattr(a.serial).unwrap();
        assert_eq!(live.size, INPUT_LENGTH);
        let start = operations(&f).len();
        let second = committed(&f, f.workspace.commit(deadline()).unwrap());
        one_edit(&operations(&f)[start..], content, metadata, INPUT_LENGTH, 4);
        assert_eq!(prepared(&operations(&f)[start..]).base, first);
        let (new_content, _) = saved(&f, second, b"dsh-largest", live);
        canonical_digest(&f, new_content, INPUT_LENGTH, &edited_hash);
        assert_eq!(publications(&f), 2);
        assert_eq!(
            operations(&f)
                .iter()
                .filter(|op| matches!(
                    op,
                    Operation::HistoryCommand(HistoryCommand::ReserveInodes { count: 1, .. })
                ))
                .count(),
            1
        );
        missing(&f, before.effective_root, b"dsh-largest");
        println!("FRESH_STREAM_INPUT bytes={offset} sha256={uploaded} caller_blocks={blocks} buffer_bytes={BLOCK} edited_sha256={edited_hash} source=preinstalled-dsh cache=undeclared-functional-only");
        drop(output);
        quiescent(&f, true);
        mount.unmount(deadline()).unwrap();
        check("pinned-DSH-mounted-upload-full-construction-then-small-canonical-edit");
        close(&f, a.serial);
    }

    fn write(f: &Fixture, handle: HandleId, offset: u64, bytes: &[u8]) {
        let payload = f.own(bytes);
        f.workspace
            .write_file(handle, offset, &payload, deadline())
            .unwrap();
    }
    /// One more replacement byte past the retired replay envelope. The bound
    /// no longer refuses: the byte publishes locally through the captured
    /// local source with no RPC, and exactly one revision advances.
    fn grow_past(f: &Fixture, serial: u64, handle: HandleId, at: u64, refresh: usize) {
        let before = f.workspace.status().unwrap();
        let calls = operations(f).len();
        write(f, handle, at, b"!");
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (after.generation, after.dirty_inodes, after.handles),
            (before.generation, before.dirty_inodes, before.handles)
        );
        assert_eq!(after.revision, before.revision + 1);
        // A local write makes no RPC of its own. After a Commit moved the
        // baseline, the first edit on a stale node refreshes its original
        // facts through the branch and attributes queries, which is the
        // designed resolution rather than write traffic.
        assert_eq!(
            operations(f).len(),
            calls + refresh,
            "unexpected write traffic; observed {:?}",
            &operations(f)[calls..]
        );
        assert_eq!(f.workspace.getattr(serial).unwrap().size, at + 1);
        assert_eq!(f.read(handle, 0, 6), b"D1tail");
    }
    #[test]
    #[ignore = "requires native captured-G delivery and exact unchanged replay admission"]
    fn fresh_stream_captured_replay() {
        let f = fixture(Gate::Delivery);
        let before = snapshot(&f);
        let (a, handle) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"captured",
                FileCreateOptions {
                    mode: 0o640,
                    umask: 0,
                    exclusive: true,
                    open: FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        append: false,
                        truncate: false,
                    },
                },
                deadline(),
            )
            .unwrap();
        write(&f, handle, 0, b"G0tail");
        let g_attrs = f.workspace.getattr(a.serial).unwrap();
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        let calls = operations(&f).len();
        write(&f, handle, 0, b"D1");
        // Captured tail contributes four Base bytes; only Local and Zero count
        // as replacement. One byte past the retired envelope still publishes
        // locally: the retired bound no longer refuses the successor's growth.
        write(&f, handle, REPLAY + 3, b"Z");
        assert_eq!(f.workspace.getattr(a.serial).unwrap().size, REPLAY + 4);
        grow_past(&f, a.serial, handle, REPLAY + 4, 0);
        assert_eq!(
            operations(&f).len(),
            calls,
            "D1 uses the captured local source, not RPC"
        );
        f.native.release();
        let stage = saving.join().unwrap().unwrap();
        let frozen = stage.stage().clone();
        assert_eq!(snapshot(&f), before);
        assert_eq!(publications(&f), 0);
        one_construct(&operations(&f), 6);
        let (g_content, g_metadata) = saved(&f, frozen.candidate_root, b"captured", g_attrs);
        assert_eq!(f.native.bytes(g_content, 0, 6), b"G0tail");
        let one = f.workspace.commit_staged(&stage, deadline()).unwrap();
        assert_eq!(one.stage_token, Some(frozen.token));
        assert_eq!(committed(&f, one), frozen.candidate_root);
        grow_past(&f, a.serial, handle, REPLAY + 5, 1);
        let live = f.workspace.getattr(a.serial).unwrap();
        let start = operations(&f).len();
        let two = committed(&f, f.workspace.commit(deadline()).unwrap());
        one_edit(
            &operations(&f)[start..],
            g_content,
            g_metadata,
            6,
            REPLAY + 2,
        );
        assert_eq!(
            prepared(&operations(&f)[start..]).base,
            frozen.candidate_root
        );
        let (content, _) = saved(&f, two, b"captured", live);
        let mut expected = Sha256::new();
        expected.update(b"D1tail");
        let zeros = [0u8; BLOCK];
        let mut remaining = REPLAY - 3;
        while remaining != 0 {
            let length = remaining.min(BLOCK as u64) as usize;
            expected.update(&zeros[..length]);
            remaining -= length as u64;
        }
        expected.update(b"Z!!");
        canonical_digest(
            &f,
            content,
            REPLAY + 6,
            &format!("{:x}", expected.finalize()),
        );
        assert_eq!(f.read(handle, 0, 6), b"D1tail");
        assert_eq!(publications(&f), 2);
        missing(&f, before.effective_root, b"captured");
        println!("FRESH_STREAM_REPLAY captured_survivor=4 local=4 zero={} replacement={} file_length={} plus1=publishes-locally before_and_after_G=true", REPLAY - 3, REPLAY + 2, REPLAY + 6);
        drop(stage);
        f.workspace.release(handle).unwrap();
        check("captured-fresh-G-tail-grows-past-the-retired-replay-limit-before-and-after-rebase");
        close(&f, a.serial);
    }
}
