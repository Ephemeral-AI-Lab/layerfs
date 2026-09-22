//! Native atomic create/open, fresh saves and existing generation reconciliation.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::{
        adapters::native::{
            client::Client, connection::connect_until, pipe, protocol::encode_request,
        },
        contract::*,
    };
    use layerfs_workspace::*;
    use std::{
        fs,
        net::ToSocketAddrs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
        time::Instant,
    };

    fn check(id: &str) {
        println!("CREATE_CHECK {id} PASS");
    }
    fn fixture_with(native: Arc<Native>, delivery: OperationDelivery) -> Fixture {
        let root =
            PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).join("create-owner");
        for path in [&root, &root.join("workspace")] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            std::os::unix::fs::chown(path, Some(1000), Some(1000)).unwrap();
            let metadata = fs::symlink_metadata(path).unwrap();
            assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
            assert_eq!(
                (metadata.uid(), metadata.gid(), metadata.mode() & 0o777),
                (1000, 1000, 0o700)
            );
        }
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root,
                max_count: 3,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            delivery,
        )
        .unwrap();
        let mut options = Fixture::options("stage", 31, WorkspaceAccess::LocalEdit);
        options.owner_uid = 1000;
        options.owner_gid = 1000;
        let workspace = host.attach(options, deadline()).unwrap();
        assert_eq!(workspace.root().uid, 1000);
        println!("MKDIR_RESOURCE create_owner_uid=1000 create_owner_gid=1000 actual_process_uid={} scope=native-configured-owner", nix::unistd::geteuid());
        Fixture {
            host,
            workspace,
            native,
        }
    }
    fn fixture(gate: Gate) -> Fixture {
        let native = Native::new_fresh(gate);
        let delivery = native.delivery();
        fixture_with(native, delivery)
    }
    fn options(
        mode: u32,
        umask: u32,
        exclusive: bool,
        access: FileAccess,
        append: bool,
        truncate: bool,
    ) -> FileCreateOptions {
        FileCreateOptions {
            mode,
            umask,
            exclusive,
            open: FileOpenOptions {
                access,
                append,
                truncate,
            },
        }
    }
    fn ordinary() -> FileCreateOptions {
        options(0o666, 0o022, true, FileAccess::ReadWrite, false, false)
    }
    fn create(f: &Fixture, name: &[u8], options: FileCreateOptions) -> (NodeAttributes, HandleId) {
        let value = f
            .workspace
            .create_file(f.workspace.root().serial, name, options, deadline())
            .unwrap();
        assert_eq!(value.0.kind, NodeKind::File);
        assert_eq!(value.0.references, 1);
        assert_eq!(value.0.uid, 1000);
        assert_eq!(value.0.gid, 1000);
        assert_eq!(f.workspace.handle_attributes(value.1).unwrap(), value.0);
        value
    }
    fn write(f: &Fixture, handle: HandleId, offset: u64, bytes: &[u8]) -> MutationReceipt {
        let payload = f.own(bytes);
        f.workspace
            .write_file(handle, offset, &payload, deadline())
            .unwrap()
    }
    fn status(f: &Fixture) -> (u64, u64, usize, usize, usize) {
        let s = f.workspace.status().unwrap();
        (s.generation, s.revision, s.dirty_inodes, s.nodes, s.handles)
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
    fn root(report: &CommitReport) -> Root {
        match &report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            CommitOutcomeWire::UpToDate { root, .. } => *root,
        }
    }
    fn commit(f: &Fixture) -> CommitReport {
        let report = if diagnostic_trace() {
            let started = Instant::now();
            let end = deadline();
            eprintln!(
                "CREATE_DIAGNOSTIC commit-start remaining_us={}",
                end.saturating_duration_since(Instant::now()).as_micros()
            );
            let result = f.workspace.commit(end);
            eprintln!(
                "CREATE_DIAGNOSTIC commit-end elapsed_us={} remaining_us={} result={result:?}",
                started.elapsed().as_micros(),
                end.saturating_duration_since(Instant::now()).as_micros()
            );
            if result.is_err() {
                eprintln!(
                    "CREATE_DIAGNOSTIC failed-commit-public-status={:?}",
                    f.workspace.status()
                );
            }
            result.unwrap()
        } else {
            f.workspace.commit(deadline()).unwrap()
        };
        assert!(f.workspace.status().unwrap().submission.is_none());
        report
    }
    fn count(f: &Fixture, predicate: impl Fn(&Operation) -> bool) -> usize {
        f.native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .filter(|op| predicate(op))
            .count()
    }
    fn reserves(f: &Fixture) -> usize {
        count(f, |op| {
            matches!(
                op,
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { .. })
            )
        })
    }
    fn publications(f: &Fixture) -> usize {
        count(f, |op| {
            matches!(
                op,
                Operation::HistoryCommand(
                    HistoryCommand::Commit(_) | HistoryCommand::CommitStaged { .. }
                )
            )
        })
    }
    fn saved(
        f: &Fixture,
        root: Root,
        name: &[u8],
        expected: &[u8],
        attrs: NodeAttributes,
    ) -> Response {
        let value = f.native.attributes(root, name);
        let Response::Attributes {
            serial,
            kind,
            size,
            mode,
            mtime,
            nanoseconds,
            content,
            references,
            ..
        } = value
        else {
            panic!("attributes")
        };
        assert_eq!(
            (serial, kind, size, mode, mtime, nanoseconds, references),
            (
                attrs.serial,
                1,
                expected.len() as u64,
                attrs.mode,
                attrs.mtime_seconds,
                attrs.mtime_nanoseconds,
                1
            )
        );
        assert_eq!(f.native.bytes(content, 0, expected.len()), expected);
        value
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
                    &mut std::io::sink()
                )
                .unwrap_err()
                .code,
            Code::PathNotFound
        );
    }
    fn close(f: &Fixture, handles: &[HandleId], refs: &[(u64, u64)]) {
        for handle in handles {
            f.workspace.release(*handle).unwrap();
        }
        for (serial, refs) in refs {
            f.workspace.forget(*serial, *refs, ReferenceScope::Local);
            assert_eq!(f.workspace.getattr(*serial), Err(WorkspaceError::NotFound));
        }
        println!(
            "MKDIR_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
        f.workspace.close_clean().unwrap();
        check("native-clean-close");
    }

    #[test]
    #[ignore = "requires create_route.py real native Service"]
    fn create_semantics() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let (empty, handle) = create(&f, b"empty", ordinary());
        assert_eq!((empty.size, empty.mode), (0, 0o644));
        assert!(f.read(handle, 0, 8).is_empty());
        let state = status(&f);
        let reserved = reserves(&f);
        assert_eq!(
            f.workspace
                .create_file(f.workspace.root().serial, b"empty", ordinary(), deadline()),
            Err(WorkspaceError::Exists)
        );
        assert_eq!(status(&f), state);
        assert_eq!(reserves(&f), reserved);
        f.workspace.forget(empty.serial, 1, ReferenceScope::Local);
        f.workspace.release(handle).unwrap();
        assert_eq!(
            f.workspace.getattr(empty.serial),
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(f.lookup(b"empty"), empty);
        let empty_handle = f
            .workspace
            .open_file(
                empty.serial,
                FileOpenOptions::default(),
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let (file, write_handle) = create(
            &f,
            b"written",
            options(0o666, 0o027, true, FileAccess::ReadWrite, false, false),
        );
        write(&f, write_handle, 0, b"native-created");
        let first_attrs = f.workspace.getattr(file.serial).unwrap();
        assert_eq!(first_attrs.mode, 0o640);
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        f.workspace.release(write_handle).unwrap();
        assert_eq!(
            f.workspace.getattr(file.serial),
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(f.lookup(b"written"), first_attrs);
        let write_handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    append: false,
                    truncate: false,
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        assert_eq!(f.read(write_handle, 0, 14), b"native-created");
        assert_eq!(publications(&f), 0);
        assert_eq!(snapshot(&f), before);
        assert_eq!(
            count(&f, |op| matches!(
                op,
                Operation::ConstructFile { .. } | Operation::ConstructPortableMetadata { .. }
            )),
            0
        );
        let first = commit(&f);
        assert_eq!(publications(&f), 1);
        saved(&f, root(&first), b"empty", b"", empty);
        saved(&f, root(&first), b"written", b"native-created", first_attrs);
        missing(&f, before.effective_root, b"empty");
        missing(&f, before.effective_root, b"written");
        let start = f.native.observations.lock().unwrap().operations.len();
        write(&f, write_handle, 0, b"NEXT");
        let second_attrs = f.workspace.getattr(file.serial).unwrap();
        let second = commit(&f);
        saved(
            &f,
            root(&second),
            b"written",
            b"NEXTve-created",
            second_attrs,
        );
        saved(&f, root(&first), b"written", b"native-created", first_attrs);
        let observed = f.native.observations.lock().unwrap();
        let lengths: Vec<_> = observed.operations[..start]
            .iter()
            .filter_map(|op| match op {
                Operation::ConstructFile { length } => Some(*length),
                _ => None,
            })
            .collect();
        assert_eq!(lengths, vec![0, 14]);
        assert_eq!(
            observed.operations[..start]
                .iter()
                .filter(|op| matches!(op, Operation::ConstructPortableMetadata { .. }))
                .count(),
            2
        );
        assert!(!observed.operations[start..].iter().any(|op| matches!(
            op,
            Operation::ConstructFile { .. } | Operation::ConstructPortableMetadata { .. }
        )));
        assert_eq!(
            observed.operations[start..]
                .iter()
                .filter(|op| matches!(op, Operation::EditFile { .. }))
                .count(),
            1
        );
        assert!(
            observed.operations[start..].iter().any(|op| matches!(op,
            Operation::HistoryCommand(HistoryCommand::Commit(p)) if p.new_file_serials.is_empty()))
        );
        drop(observed);
        assert_eq!(reserves(&f), 2);
        check("atomic-create-handle-forget-relookup-zero-save-and-existing-next-Commit");
        close(
            &f,
            &[empty_handle, write_handle],
            &[(empty.serial, 1), (file.serial, 1)],
        );
    }

    #[test]
    #[ignore = "requires genuine uid1000 host and public permission manifest"]
    fn create_flags_permissions() {
        let f = fixture(Gate::None);
        let mut handles = Vec::new();
        let mut refs = Vec::new();
        let mut expected = Vec::new();
        for (name, mode, access, append) in [
            (b"mode0400".as_slice(), 0o400, FileAccess::ReadWrite, false),
            (b"mode000".as_slice(), 0, FileAccess::WriteOnly, true),
        ] {
            let (a, handle) = create(&f, name, options(mode, 0, true, access, append, false));
            write(&f, handle, 0, b"abc");
            write(&f, handle, 0, b"DEF");
            if append {
                assert!(matches!(
                    f.workspace.read(handle, 0, 0, deadline()),
                    Err(WorkspaceError::BadHandle)
                ));
            } else {
                assert_eq!(f.read(handle, 0, 3), b"DEF");
            }
            assert_eq!(
                f.workspace.open_file(
                    a.serial,
                    FileOpenOptions {
                        access: FileAccess::WriteOnly,
                        append: false,
                        truncate: false
                    },
                    ReferenceScope::Local,
                    deadline()
                ),
                Err(WorkspaceError::Denied)
            );
            assert_eq!(
                f.workspace.set_len(a.serial, 0, deadline()),
                Err(WorkspaceError::Denied)
            );
            let payload = f.own(b"x");
            assert!(matches!(
                f.workspace.edit_file_range(
                    &WorkspacePath::new(name).unwrap(),
                    &RangeEdit {
                        start: 0,
                        end: 0,
                        replacement: payload
                    },
                    deadline()
                ),
                Err(WorkspaceError::Denied)
            ));
            expected.push((
                name,
                if append {
                    b"abcDEF".as_slice()
                } else {
                    b"DEF".as_slice()
                },
                f.workspace.getattr(a.serial).unwrap(),
            ));
            handles.push(handle);
            refs.push((a.serial, 1));
        }
        let parent = f.lookup(b"search-only");
        assert_eq!(parent.mode, 0o500);
        let reserved = reserves(&f);
        let parent_before = f.workspace.getattr(parent.serial).unwrap();
        let (existing, existing_handle) = f
            .workspace
            .create_file(
                parent.serial,
                b"existing",
                options(0, 0o777, false, FileAccess::ReadWrite, false, true),
                deadline(),
            )
            .unwrap();
        assert_eq!((existing.mode, existing.size), (0o600, 0));
        assert!(f.read(existing_handle, 0, 1).is_empty());
        let (again, second_handle) = f
            .workspace
            .create_file(
                parent.serial,
                b"existing",
                options(0o777, 0, false, FileAccess::ReadOnly, false, false),
                deadline(),
            )
            .unwrap();
        assert_eq!(again, existing);
        assert_eq!(f.workspace.getattr(parent.serial).unwrap(), parent_before);
        assert_eq!(reserves(&f), reserved);
        let before_refusals = status(&f);
        assert_eq!(
            f.workspace
                .create_file(parent.serial, b"existing", ordinary(), deadline()),
            Err(WorkspaceError::Exists)
        );
        assert_eq!(
            f.workspace
                .create_file(parent.serial, b"new", ordinary(), deadline()),
            Err(WorkspaceError::Denied)
        );
        assert_eq!(
            f.workspace.create_file(
                f.workspace.root().serial,
                b"search-only",
                options(0o600, 0, false, FileAccess::ReadOnly, false, false),
                deadline()
            ),
            Err(WorkspaceError::IsDirectory)
        );
        assert_eq!(
            f.workspace.create_file(
                f.workspace.root().serial,
                b"link",
                options(0o600, 0, false, FileAccess::ReadOnly, false, false),
                deadline()
            ),
            Err(WorkspaceError::WrongKind)
        );
        assert_eq!(reserves(&f), reserved);
        assert_eq!(status(&f), before_refusals);
        let report = commit(&f);
        for (name, bytes, attrs) in expected {
            saved(&f, root(&report), name, bytes, attrs);
        }
        saved(&f, root(&report), b"search-only/existing", b"", existing);
        handles.extend([existing_handle, second_handle]);
        refs.extend([(existing.serial, 2), (parent.serial, 1)]);
        check("creation-fd-rights-mode0400-mode000-and-existing-search-only-parent");
        close(&f, &handles, &refs);
    }

    #[test]
    #[ignore = "requires fresh-file delivery gate and actual C5 staged reconciliation"]
    fn create_successor() {
        let f = fixture(Gate::Delivery);
        let before = snapshot(&f);
        let (a, handle) = create(&f, b"captured", ordinary());
        write(&f, handle, 0, b"G0tail");
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        write(&f, handle, 0, b"D1");
        assert_eq!(f.read(handle, 0, 6), b"D1tail");
        f.native.release();
        let stage = saving.join().unwrap().unwrap();
        let frozen = stage.stage().clone();
        let g = f.native.attributes(frozen.candidate_root, b"captured");
        let Response::Attributes {
            content: g_content,
            metadata: g_metadata,
            ..
        } = g
        else {
            panic!("G attributes")
        };
        assert_eq!(f.native.bytes(g_content, 0, 6), b"G0tail");
        let (born, born_handle) = create(&f, b"born", ordinary());
        write(&f, born_handle, 0, b"born-data");
        let a_live = f.workspace.getattr(a.serial).unwrap();
        let born_live = f.workspace.getattr(born.serial).unwrap();
        let one = f.workspace.commit_staged(&stage, deadline()).unwrap();
        assert_eq!(root(&one), frozen.candidate_root);
        assert_eq!(one.stage_token, Some(frozen.token));
        assert!(f.workspace.status().unwrap().submission.is_none());
        assert_eq!(f.read(handle, 0, 6), b"D1tail");
        assert_eq!(f.read(born_handle, 0, 9), b"born-data");
        missing(&f, root(&one), b"born");
        let start = f.native.observations.lock().unwrap().operations.len();
        let two = commit(&f);
        saved(&f, root(&two), b"captured", b"D1tail", a_live);
        saved(&f, root(&two), b"born", b"born-data", born_live);
        let observed = f.native.observations.lock().unwrap();
        assert!(observed.operations[start..]
            .iter()
            .any(|op| matches!(op, Operation::EditFile { root, .. } if *root == g_content)));
        assert!(observed.operations[start..].iter().any(
            |op| matches!(op, Operation::UpdatePortableMetadata { base, .. } if *base == g_metadata)
        ));
        assert_eq!(
            observed.operations[start..]
                .iter()
                .filter(|op| matches!(op, Operation::ConstructFile { length: 9 }))
                .count(),
            1
        );
        assert_eq!(
            observed.operations[start..]
                .iter()
                .filter(|op| matches!(op, Operation::ConstructPortableMetadata { .. }))
                .count(),
            1
        );
        let prepared = observed.operations[start..]
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p),
                _ => None,
            })
            .unwrap();
        assert_eq!(prepared.new_file_serials, vec![born.serial]);
        assert_eq!(prepared.base, frozen.candidate_root);
        drop(observed);
        drop(stage);
        assert_eq!(reserves(&f), 2);
        assert_eq!(publications(&f), 2);
        missing(&f, before.effective_root, b"captured");
        missing(&f, before.effective_root, b"born");
        check("captured-fresh-file-D1-and-D1-born-file-reconcile-both-saved-roots");
        close(
            &f,
            &[handle, born_handle],
            &[(a.serial, 1), (born.serial, 1)],
        );
    }

    fn name(index: usize) -> Vec<u8> {
        format!("f{index:03}-{}", "x".repeat(250)).into_bytes()
    }
    fn encoded(
        snapshot: &BranchSnapshotWire,
        existing: InodeChange,
        count: usize,
    ) -> Result<Vec<u8>, Failure> {
        let mut inodes = vec![existing];
        inodes.extend((0..count).map(|i| InodeChange {
            serial: 1000 + i as u64,
            kind: 1,
            content: [1; 32],
            metadata: [2; 32],
        }));
        let p = PreparedChanges {
            workspace: [31; 32],
            branch: snapshot.branch.branch,
            expected_head: snapshot.branch.head_commit,
            expected_base: snapshot.branch.base_layer,
            generation: 1,
            base: snapshot.effective_root,
            scope: snapshot.scope,
            root_serial: snapshot.root_serial.unwrap(),
            directories: vec![DirectoryChange {
                parent: snapshot.root_serial.unwrap(),
                changes: (0..count)
                    .map(|i| (name(i), Some(1000 + i as u64)))
                    .collect(),
            }],
            inodes,
            new_directories: vec![],
            directory_metadata: vec![DirectoryMetadata {
                serial: snapshot.root_serial.unwrap(),
                mode: 0o755,
                mtime_seconds: 1,
                mtime_nanoseconds: 0,
            }],
            new_file_serials: (0..count).map(|i| 1000 + i as u64).collect(),
            new_symlink_serials: Vec::new(),
        };
        encode_prepared(p)
    }
    fn encode_prepared(p: PreparedChanges) -> Result<Vec<u8>, Failure> {
        encode_request(&Request {
            id: 1,
            generation: 1,
            store: 1,
            profile: HISTORY_PROFILE,
            deadline_ms: 10000,
            response_bytes: 0,
            operation: Operation::HistoryCommand(HistoryCommand::Commit(p)),
        })
    }
    #[test]
    #[ignore = "requires fixed long names and full admitted fresh-file frontier"]
    fn create_capacity() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let data = f.lookup(b"data.bin");
        let Response::Attributes {
            serial,
            kind,
            content,
            metadata,
            ..
        } = f.native.attributes(before.effective_root, b"data.bin")
        else {
            panic!("old file")
        };
        let existing = InodeChange {
            serial,
            kind,
            content,
            metadata,
        };
        let expected = (1..=127)
            .take_while(|n| encoded(&before, existing, *n).is_ok())
            .last()
            .unwrap();
        assert!((32..126).contains(&expected));
        let bytes = encoded(&before, existing, expected).unwrap().len();
        assert_eq!(
            encoded(&before, existing, expected + 1).unwrap_err().code,
            Code::Capacity
        );
        assert!(bytes <= 32768 && bytes + 346 > 32768);
        f.edit(b"data.bin", 0, 4, b"EDIT");
        let edited = f.workspace.getattr(data.serial).unwrap();
        let mut accepted = Vec::new();
        for index in 0..expected {
            let (a, handle) = create(&f, &name(index), ordinary());
            if index == 0 {
                write(&f, handle, 8 * 1024 * 1024 - 1, b"Z");
                let attrs = f.workspace.getattr(a.serial).unwrap();
                let before_refusal = status(&f);
                let payload = f.own(b"x");
                assert_eq!(
                    f.workspace
                        .write_file(handle, 8 * 1024 * 1024, &payload, deadline()),
                    Err(WorkspaceError::Capacity)
                );
                assert_eq!(f.workspace.getattr(a.serial).unwrap(), attrs);
                assert_eq!(status(&f), before_refusal);
                for offset in [0, 4 * 1024 * 1024] {
                    assert_eq!(f.read(handle, offset, 32), vec![0; 32]);
                }
                assert_eq!(f.read(handle, 8 * 1024 * 1024 - 2, 2), [0, b'Z']);
            }
            accepted.push(f.workspace.getattr(a.serial).unwrap());
            f.workspace.release(handle).unwrap();
            f.workspace.forget(a.serial, 1, ReferenceScope::Local);
            assert_eq!(f.workspace.getattr(a.serial), Err(WorkspaceError::NotFound));
        }
        let full = status(&f);
        assert_eq!(reserves(&f), expected);
        assert_eq!(
            f.workspace.create_file(
                f.workspace.root().serial,
                &name(expected),
                ordinary(),
                deadline()
            ),
            Err(WorkspaceError::Capacity)
        );
        assert_eq!(status(&f), full);
        assert_eq!(reserves(&f), expected);
        assert_eq!(publications(&f), 0);
        let report = commit(&f);
        assert_eq!(publications(&f), 1);
        let observed = f.native.observations.lock().unwrap();
        let prepared = observed
            .operations
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(prepared.inodes.len(), expected + 1);
        assert_eq!(prepared.new_file_serials.len(), expected);
        assert_eq!(prepared.directory_metadata.len(), 1);
        assert_eq!(encode_prepared(prepared).unwrap().len(), bytes);
        let constructed: Vec<_> = observed
            .operations
            .iter()
            .filter_map(|op| match op {
                Operation::ConstructFile { length } => Some(*length),
                _ => None,
            })
            .collect();
        assert_eq!(constructed.len(), expected);
        assert_eq!(
            constructed
                .iter()
                .filter(|n| **n == 8 * 1024 * 1024)
                .count(),
            1
        );
        assert!(constructed.iter().all(|n| *n == 0 || *n == 8 * 1024 * 1024));
        drop(observed);
        for (index, attrs) in accepted.iter().enumerate() {
            if index == 0 {
                let value = f.native.attributes(root(&report), &name(index));
                assert!(
                    matches!(&value, Response::Attributes { serial, mode, mtime, nanoseconds, .. }
                    if *serial == attrs.serial && *mode == attrs.mode && *mtime == attrs.mtime_seconds && *nanoseconds == attrs.mtime_nanoseconds)
                );
                let info = attr(value);
                assert_eq!(info.2, 8 * 1024 * 1024);
                for offset in [0, 4 * 1024 * 1024] {
                    assert_eq!(f.native.bytes(info.1, offset, 32), vec![0; 32]);
                }
                assert_eq!(f.native.bytes(info.1, 8 * 1024 * 1024 - 2, 2), [0, b'Z']);
            } else {
                saved(&f, root(&report), &name(index), b"", *attrs);
            }
        }
        let value = f.native.attributes(root(&report), b"data.bin");
        assert!(
            matches!(&value, Response::Attributes { serial, mode, mtime, nanoseconds, .. }
            if *serial == edited.serial && *mode == edited.mode && *mtime == edited.mtime_seconds && *nanoseconds == edited.mtime_nanoseconds)
        );
        let stored = attr(value);
        assert_eq!(stored.2, 64 * 1024 * 1024);
        assert_eq!(f.native.bytes(stored.1, 0, 4), b"EDIT");
        println!("MKDIR_CAPACITY create_accepted={expected} encoded_bytes={bytes} next_bytes={} name_bytes=255 files={} dirs=1 fresh={} limit=32768 replay_limit=8388608 full_ConstructFile_bytes=8388608 readback=three-declared-windows", bytes + 346, expected + 1, expected);
        check("exact-fresh-file-wire-frontier-and-eight-MiB-envelope-refuse-atomically");
        close(&f, &[], &[(data.serial, 1)]);
    }

    #[test]
    #[ignore = "requires native owner-permission and unbound projection refusal"]
    fn create_refusals() {
        let f = fixture(Gate::None);
        let parent = f.workspace.root().serial;
        let initial = status(&f);
        for bad in [
            b"".as_slice(),
            b".",
            b"..",
            b"a/b",
            b"a\\b",
            b"a\0b",
            &[0xff],
            &[b'x'; 256],
        ] {
            assert_eq!(
                f.workspace.create_file(parent, bad, ordinary(), deadline()),
                Err(WorkspaceError::InvalidInput)
            );
        }
        for bad in [
            options(0o1000, 0, true, FileAccess::ReadWrite, false, false),
            options(0o600, 0o1000, true, FileAccess::ReadWrite, false, false),
            options(0o600, 0, true, FileAccess::ReadOnly, true, false),
            options(0o600, 0, true, FileAccess::ReadOnly, false, true),
        ] {
            assert_eq!(
                f.workspace.create_file(parent, b"bad", bad, deadline()),
                Err(WorkspaceError::InvalidInput)
            );
        }
        assert_eq!(
            f.workspace
                .create_file(parent, b"late", ordinary(), Instant::now()),
            Err(WorkspaceError::Deadline)
        );
        assert_eq!(status(&f), initial);
        assert_eq!(reserves(&f), 0);
        let data = f.lookup(b"data.bin");
        let search = f.lookup(b"search-only");
        assert_eq!(
            f.workspace
                .create_file(data.serial, b"child", ordinary(), deadline()),
            Err(WorkspaceError::NotDirectory)
        );
        assert_eq!(
            f.workspace
                .create_file(search.serial, b"child", ordinary(), deadline()),
            Err(WorkspaceError::Denied)
        );
        assert_eq!(
            f.workspace
                .create_file(parent, b"link", ordinary(), deadline()),
            Err(WorkspaceError::Exists)
        );
        let nonexclusive = options(0o600, 0, false, FileAccess::ReadOnly, false, false);
        assert_eq!(
            f.workspace
                .create_file(parent, b"link", nonexclusive, deadline()),
            Err(WorkspaceError::WrongKind)
        );
        assert_eq!(
            f.workspace
                .create_file(parent, b"search-only", nonexclusive, deadline()),
            Err(WorkspaceError::IsDirectory)
        );
        let mut ro_options = Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly);
        ro_options.owner_uid = 1000;
        ro_options.owner_gid = 1000;
        let ro = f.host.attach(ro_options, deadline()).unwrap();
        assert_eq!(
            ro.create_file(ro.root().serial, b"no", ordinary(), deadline()),
            Err(WorkspaceError::ReadOnly)
        );
        ro.close_clean().unwrap();
        // An acquired but unbound projection excludes native creation before
        // reservation; actual mounted creation is covered by mounted_create.
        let projected = Fixture::new(Gate::None);
        assert_eq!(projected.workspace.root().uid, 0);
        let projected_root = PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap());
        for path in [&projected_root, &projected_root.join("workspace")] {
            assert_eq!(fs::symlink_metadata(path).unwrap().uid(), 0);
        }
        let mut mount = projected.workspace.reserve_mount().unwrap();
        assert!(projected.workspace.status().unwrap().mounted);
        assert_eq!(
            projected.workspace.create_file(
                projected.workspace.root().serial,
                b"mounted",
                ordinary(),
                deadline()
            ),
            Err(WorkspaceError::Busy)
        );
        assert_eq!(reserves(&projected), 0);
        assert_eq!(publications(&projected), 0);
        mount.finish().unwrap();
        assert!(!projected.workspace.status().unwrap().mounted);
        projected.workspace.close_clean().unwrap();
        assert_eq!(reserves(&f), 0);
        assert_eq!(publications(&f), 0);
        let denied = f
            .workspace
            .mkdir(parent, b"no-search", 0o600, 0, deadline())
            .unwrap();
        let before = status(&f);
        let reserved = reserves(&f);
        assert_eq!(
            f.workspace
                .create_file(denied.serial, b"child", ordinary(), deadline()),
            Err(WorkspaceError::Denied)
        );
        assert_eq!(status(&f), before);
        assert_eq!(reserves(&f), reserved);
        commit(&f);
        check("native-create-refusals-preserve-namespace-handles-and-reservations");
        close(
            &f,
            &[],
            &[(data.serial, 1), (search.serial, 1), (denied.serial, 1)],
        );
    }

    fn reserve(native: &Native, scope: Root) -> u64 {
        let response = native
            .request(
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
                0,
                &mut std::io::sink(),
            )
            .unwrap();
        let Response::History(value) = response else {
            panic!("history")
        };
        let HistoryResult::Reservation {
            scope: actual,
            start,
            count: 1,
        } = *value
        else {
            panic!("reservation")
        };
        assert_eq!(actual, scope);
        start
    }
    fn reserve_failure(unknown: bool) {
        let native = Native::new_fresh(Gate::None);
        let failing = Arc::new(AtomicBool::new(true));
        let attempts = Arc::new(AtomicUsize::new(0));
        let n = native.clone();
        let flag = failing.clone();
        let calls = attempts.clone();
        let delivery: OperationDelivery = Arc::new(move |request, input, output, end| {
            if flag.load(Ordering::Acquire)
                && matches!(
                    request.operation,
                    Operation::HistoryCommand(HistoryCommand::ReserveInodes { .. })
                )
            {
                calls.fetch_add(1, Ordering::AcqRel);
                n.observations
                    .lock()
                    .unwrap()
                    .operations
                    .push(request.operation.clone());
                let (endpoint, selector, private) = if unknown {
                    (
                        std::env::var("LAYERFS_RESERVE_ENDPOINT")
                            .unwrap()
                            .to_socket_addrs()
                            .unwrap()
                            .next()
                            .unwrap(),
                        1,
                        n.private,
                    )
                } else {
                    (
                        n.endpoint,
                        2,
                        pipe::key(&std::env::var("LAYERFS_RESERVE_PRIVATE_KEY").unwrap()).unwrap(),
                    )
                };
                let mut client =
                    Client::new(connect_until(endpoint, selector, &private, &n.server, end)?)?;
                client.call_until(request, input, output, end)
            } else {
                n.call(request, input, output, end)
            }
        });
        let f = fixture_with(native, delivery);
        let before = snapshot(&f);
        let marker = reserve(&f.native, before.scope);
        let initial = status(&f);
        let parent = f.workspace.getattr(f.workspace.root().serial).unwrap();
        let error = f
            .workspace
            .create_file(parent.serial, b"failed", ordinary(), deadline())
            .unwrap_err();
        assert!(
            matches!(&error, WorkspaceError::Service(failure) if failure.unknown == unknown && (unknown || failure.code == Code::Denied)),
            "{error:?}"
        );
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        assert_eq!(status(&f), initial);
        assert_eq!(f.workspace.getattr(parent.serial).unwrap(), parent);
        assert_eq!(snapshot(&f), before);
        assert!(matches!(
            f.workspace
                .lookup(parent.serial, b"failed", ReferenceScope::Local, deadline()),
            Err(WorkspaceError::NotFound)
                | Err(WorkspaceError::Service(Failure {
                    code: Code::PathNotFound,
                    ..
                }))
        ));
        let next = reserve(&f.native, before.scope);
        assert_eq!(next, marker + if unknown { 2 } else { 1 });
        failing.store(false, Ordering::Release);
        let (after, handle) = create(&f, b"after", ordinary());
        assert_eq!(after.serial, next + 1);
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        let report = commit(&f);
        saved(&f, root(&report), b"after", b"", after);
        missing(&f, root(&report), b"failed");
        println!("MKDIR_RESERVE create_unknown={unknown} attempts=1 before={marker} next={next} successful={} {error:?}", after.serial);
        check(if unknown {
            "create-unknown-Reserve-consumed-once-no-handle-name-or-replay"
        } else {
            "create-denied-Reserve-no-handle-name-or-replay"
        });
        close(&f, &[handle], &[(after.serial, 1)]);
    }
    #[test]
    #[ignore = "requires actual Service denied principal"]
    fn create_reserve_denied() {
        reserve_failure(false);
    }
    #[test]
    #[ignore = "requires actual encrypted Reserve terminal withholding"]
    fn create_reserve_unknown() {
        reserve_failure(true);
    }
}
