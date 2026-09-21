//! Native Workspace namespace mutations; mounted mkdir remains explicitly refused.
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
        collections::BTreeMap,
        net::ToSocketAddrs,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
        time::Instant,
    };

    fn check(name: &str) {
        println!("MKDIR_CHECK {name} PASS");
    }
    fn snapshot(f: &Fixture) -> BranchSnapshotWire {
        let Response::History(result) = f.branch() else {
            panic!("history")
        };
        let HistoryResult::BranchSnapshot(value) = *result else {
            panic!("branch")
        };
        value
    }
    fn commit(f: &Fixture) -> CommitReport {
        let report = f.workspace.commit(deadline()).unwrap();
        assert!(f.workspace.status().unwrap().submission.is_none());
        report
    }
    fn root(report: &CommitReport) -> Root {
        match &report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            CommitOutcomeWire::UpToDate { root, .. } => *root,
        }
    }
    fn calls(f: &Fixture, predicate: impl Fn(&Operation) -> bool) -> usize {
        f.native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .filter(|op| predicate(op))
            .count()
    }
    fn reservations(f: &Fixture) -> usize {
        calls(f, |op| {
            matches!(
                op,
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { .. })
            )
        })
    }
    fn publications(f: &Fixture) -> usize {
        calls(f, |op| {
            matches!(
                op,
                Operation::HistoryCommand(
                    HistoryCommand::Commit(_) | HistoryCommand::CommitStaged { .. }
                )
            )
        })
    }
    fn state(workspace: &Workspace) -> (u64, u64, usize) {
        let s = workspace.status().unwrap();
        (s.generation, s.revision, s.dirty_inodes)
    }
    fn mkdir(f: &Fixture, parent: u64, name: &[u8], mode: u32, umask: u32) -> NodeAttributes {
        let a = f
            .workspace
            .mkdir(parent, name, mode, umask, deadline())
            .unwrap();
        assert_eq!(
            (a.kind, a.size, a.references, a.mode),
            (NodeKind::Directory, 0, 1, mode & !umask)
        );
        assert_eq!(a, f.workspace.getattr(a.serial).unwrap());
        a
    }
    fn lookup(workspace: &Workspace, parent: u64, name: &[u8]) -> NodeAttributes {
        workspace
            .lookup(parent, name, ReferenceScope::Local, deadline())
            .unwrap()
    }
    fn listing(workspace: &Workspace, serial: u64) -> BTreeMap<Vec<u8>, u64> {
        let nodes_before = workspace.status().unwrap().nodes;
        let handle = workspace.opendir(serial, ReferenceScope::Local).unwrap();
        let mut cookie = 0;
        let mut all = BTreeMap::new();
        loop {
            let page = workspace.readdir(handle, cookie, 17, deadline()).unwrap();
            if page.entries().is_empty() {
                break;
            }
            for item in page.entries() {
                assert_ne!(item.cookie, cookie);
                cookie = item.cookie;
                assert!(all.insert(item.name.clone(), item.serial).is_none());
            }
            assert!(all.len() <= 132);
        }
        workspace.releasedir(handle).unwrap();
        assert_eq!(
            workspace.status().unwrap().nodes,
            nodes_before,
            "readdir adds no public lookup references"
        );
        all
    }
    fn tail(
        workspace: &Workspace,
        handle: HandleId,
        start: u64,
    ) -> Vec<(Vec<u8>, u64, NodeKind, u64)> {
        let mut cookie = start;
        let mut values = Vec::new();
        loop {
            let page = workspace.readdir(handle, cookie, 17, deadline()).unwrap();
            if page.entries().is_empty() {
                break;
            }
            for entry in page.entries() {
                assert_ne!(entry.cookie, cookie);
                cookie = entry.cookie;
                values.push((entry.name.clone(), entry.serial, entry.kind, entry.cookie));
            }
            assert!(values.len() < 16);
        }
        values
    }
    fn missing(f: &Fixture, root: Root, path: &[u8]) {
        let error = f
            .native
            .request(
                Operation::Inspect {
                    root,
                    query: Inspect::Attributes {
                        path: path.to_vec(),
                    },
                },
                0,
                &mut std::io::sink(),
            )
            .unwrap_err();
        assert_eq!(error.code, Code::PathNotFound);
    }
    fn saved_attributes(f: &Fixture, root: Root, path: &[u8], expected: NodeAttributes) {
        let Response::Attributes {
            serial,
            kind,
            references,
            mode,
            mtime,
            nanoseconds,
            size,
            ..
        } = f.native.attributes(root, path)
        else {
            panic!("attributes")
        };
        assert_eq!((serial, kind, references, size), (expected.serial, 2, 1, 0));
        assert_eq!(
            (mode, mtime, nanoseconds),
            (
                expected.mode,
                expected.mtime_seconds,
                expected.mtime_nanoseconds
            )
        );
    }
    fn close(f: &Fixture, serials: &[u64]) {
        for serial in serials {
            f.workspace.forget(*serial, u64::MAX, ReferenceScope::Local);
        }
        println!(
            "MKDIR_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
        f.workspace.close_clean().unwrap();
        assert!(f.workspace.status().unwrap().closed);
        check("native-clean-close");
    }

    #[test]
    #[ignore = "requires mkdir_route.py live native Service"]
    fn mkdir_semantics() {
        let f = Fixture::new(Gate::None);
        let before = snapshot(&f);
        let parent = f.workspace.root().serial;
        let old_root_attributes = f.native.attributes(before.effective_root, b"");
        let old_handle = f.workspace.opendir(parent, ReferenceScope::Local).unwrap();
        let first_page = f.workspace.readdir(old_handle, 0, 2, deadline()).unwrap();
        assert_eq!(
            first_page
                .entries()
                .iter()
                .map(|e| e.name.as_slice())
                .collect::<Vec<_>>(),
            vec![b".".as_slice(), b"..".as_slice()]
        );
        let old_cookie = first_page.entries()[1].cookie;
        drop(first_page);
        let old_tail = tail(&f.workspace, old_handle, old_cookie);
        assert_eq!(old_tail.len(), 3);
        let a = mkdir(&f, parent, b"new", 0o1777, 0o027);
        let after = state(&f.workspace);
        let count = reservations(&f);
        assert_eq!(
            f.workspace.mkdir(parent, b"new", 0o755, 0, deadline()),
            Err(WorkspaceError::Exists)
        );
        assert_eq!(state(&f.workspace), after);
        assert_eq!(reservations(&f), count);
        f.workspace.forget(a.serial, 1, ReferenceScope::Local);
        assert_eq!(f.workspace.getattr(a.serial), Err(WorkspaceError::NotFound));
        assert_eq!(lookup(&f.workspace, parent, b"new"), a);
        let b = mkdir(&f, a.serial, "nested-目录".as_bytes(), 0o777, 0o077);
        assert_eq!(
            listing(&f.workspace, parent).get(b"new".as_slice()),
            Some(&a.serial)
        );
        assert_eq!(
            listing(&f.workspace, a.serial).get("nested-目录".as_bytes()),
            Some(&b.serial)
        );
        f.workspace.forget(b.serial, 1, ReferenceScope::Local);
        assert_eq!(
            f.workspace.getattr(b.serial),
            Err(WorkspaceError::NotFound),
            "readdir must not add a lookup reference to an already cached child"
        );
        assert_eq!(lookup(&f.workspace, a.serial, "nested-目录".as_bytes()), b);
        assert_eq!(tail(&f.workspace, old_handle, old_cookie), old_tail);
        assert_eq!(publications(&f), 0);
        assert_eq!(snapshot(&f), before);
        let first = commit(&f);
        assert_eq!(publications(&f), 1);
        assert_eq!(tail(&f.workspace, old_handle, old_cookie), old_tail);
        assert_eq!(
            f.native.attributes(before.effective_root, b""),
            old_root_attributes
        );
        saved_attributes(&f, root(&first), "new/nested-目录".as_bytes(), b);
        missing(&f, before.effective_root, b"new");
        let parent_attr = f.workspace.getattr(a.serial).unwrap();
        saved_attributes(&f, root(&first), b"new", parent_attr);
        let c = mkdir(&f, a.serial, b"second", 0o755, 0o022);
        missing(&f, root(&first), b"new/second");
        assert_eq!(publications(&f), 1);
        let second = commit(&f);
        assert_eq!(publications(&f), 2);
        saved_attributes(&f, root(&second), b"new/second", c);
        saved_attributes(&f, root(&second), "new/nested-目录".as_bytes(), b);
        assert_eq!(reservations(&f), 3);
        for op in &f.native.observations.lock().unwrap().operations {
            if let Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count }) = op {
                assert_eq!(*scope, before.scope);
                assert_eq!(*count, 1);
            }
        }
        f.workspace.forget(c.serial, 1, ReferenceScope::Local);
        assert_eq!(lookup(&f.workspace, a.serial, b"second"), c);
        assert_eq!(tail(&f.workspace, old_handle, old_cookie), old_tail);
        f.workspace.releasedir(old_handle).unwrap();
        for serial in [a.serial, b.serial, c.serial] {
            f.workspace.forget(serial, 1, ReferenceScope::Local);
            assert_eq!(f.workspace.getattr(serial), Err(WorkspaceError::NotFound));
        }
        check("nested-mode-umask-forget-listing-and-two-explicit-generations");
        close(&f, &[]);
    }

    #[test]
    #[ignore = "requires exact captured new directory and native C5"]
    fn mkdir_successor() {
        let f = Fixture::new(Gate::None);
        let before = snapshot(&f);
        let a = mkdir(&f, f.workspace.root().serial, b"captured", 0o755, 0);
        let stage = f.workspace.stage(deadline()).unwrap();
        let frozen = stage.stage().clone();
        assert_eq!(publications(&f), 0);
        let b = mkdir(&f, a.serial, b"successor", 0o700, 0);
        assert_eq!(
            f.native.query(HistoryQuery::GetStage {
                workspace: [31; 32]
            }),
            Response::History(Box::new(HistoryResult::Stage(frozen.clone())))
        );
        missing(&f, frozen.candidate_root, b"captured/successor");
        saved_attributes(&f, frozen.candidate_root, b"captured", a);
        let live = f.workspace.getattr(a.serial).unwrap();
        let first = f.workspace.commit_staged(&stage, deadline()).unwrap();
        assert_eq!(root(&first), frozen.candidate_root);
        assert_eq!(first.generation, frozen.generation);
        assert_eq!(first.stage_token, Some(frozen.token));
        assert!(f.workspace.status().unwrap().submission.is_none());
        assert!(f.workspace.status().unwrap().dirty_inodes > 0);
        assert_eq!(lookup(&f.workspace, a.serial, b"successor"), b);
        assert_eq!(f.workspace.getattr(a.serial).unwrap(), live);
        let second = commit(&f);
        saved_attributes(&f, root(&second), b"captured/successor", b);
        saved_attributes(&f, root(&second), b"captured", live);
        missing(&f, before.effective_root, b"captured");
        let operations = f.native.observations.lock().unwrap();
        let prepared: Vec<_> = operations
            .operations
            .iter()
            .filter_map(|op| match op {
                Operation::HistoryCommand(
                    HistoryCommand::StageChanges(p) | HistoryCommand::Commit(p),
                ) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(prepared.len(), 2);
        assert_eq!(
            prepared[0]
                .new_directories
                .iter()
                .map(|d| d.serial)
                .collect::<Vec<_>>(),
            vec![a.serial]
        );
        assert_eq!(
            prepared[1]
                .new_directories
                .iter()
                .map(|d| d.serial)
                .collect::<Vec<_>>(),
            vec![b.serial]
        );
        assert_eq!(prepared[1].base, frozen.candidate_root);
        assert_eq!(
            prepared[1]
                .directory_metadata
                .iter()
                .map(|p| p.serial)
                .collect::<Vec<_>>(),
            vec![a.serial]
        );
        assert!(prepared[1].inodes.is_empty());
        assert_eq!(
            prepared[1].directories,
            vec![
                DirectoryChange {
                    parent: a.serial,
                    changes: vec![(b"successor".to_vec(), Some(b.serial))]
                },
                DirectoryChange {
                    parent: b.serial,
                    changes: vec![]
                }
            ]
        );
        drop(operations);
        drop(stage);
        assert_eq!(publications(&f), 2);
        assert_eq!(reservations(&f), 2);
        check("captured-new-parent-retains-D1-through-CommitStaged-and-next-Commit");
        close(&f, &[a.serial, b.serial]);
    }

    fn long_name(index: usize) -> Vec<u8> {
        format!("n{index:03}-{}", "x".repeat(250)).into_bytes()
    }
    fn encoded(
        snapshot: &BranchSnapshotWire,
        file: InodeChange,
        names: usize,
    ) -> Result<Vec<u8>, Failure> {
        let first = 1000u64;
        let mut directories = vec![DirectoryChange {
            parent: snapshot.root_serial.unwrap(),
            changes: (0..names)
                .map(|i| (long_name(i), Some(first + i as u64)))
                .collect(),
        }];
        directories.extend((0..names).map(|i| DirectoryChange {
            parent: first + i as u64,
            changes: vec![],
        }));
        let new_directories = (0..names)
            .map(|i| DirectoryMetadata {
                serial: first + i as u64,
                mode: 0o755,
                mtime_seconds: 1,
                mtime_nanoseconds: 0,
            })
            .collect();
        let changes = PreparedChanges {
            workspace: [31; 32],
            branch: snapshot.branch.branch,
            expected_head: snapshot.branch.head_commit,
            expected_base: snapshot.branch.base_layer,
            generation: 1,
            base: snapshot.effective_root,
            scope: snapshot.scope,
            root_serial: snapshot.root_serial.unwrap(),
            directories,
            inodes: vec![file],
            new_directories,
            directory_metadata: vec![DirectoryMetadata {
                serial: snapshot.root_serial.unwrap(),
                mode: 0o755,
                mtime_seconds: 1,
                mtime_nanoseconds: 0,
            }],
        };
        encode_request(&Request {
            id: 1,
            generation: 1,
            store: 1,
            profile: HISTORY_PROFILE,
            deadline_ms: 10000,
            response_bytes: 0,
            operation: Operation::HistoryCommand(HistoryCommand::Commit(changes)),
        })
    }

    #[test]
    #[ignore = "requires fixed 255-byte names, mixed file edit and native Commit"]
    fn mkdir_capacity() {
        let f = Fixture::new(Gate::None);
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
            panic!("file")
        };
        let file = InodeChange {
            serial,
            kind,
            content,
            metadata,
        };
        let expected = (1..=127)
            .take_while(|n| encoded(&before, file, *n).is_ok())
            .last()
            .unwrap();
        let wire_bytes = encoded(&before, file, expected).unwrap().len();
        assert!((32..126).contains(&expected));
        assert_eq!(
            encoded(&before, file, expected + 1).unwrap_err().code,
            Code::Capacity
        );
        assert!(wire_bytes <= 32768 && wire_bytes + 299 > 32768);
        f.edit(b"data.bin", 0, 4, b"DIRS");
        let mut accepted = Vec::new();
        let initial_pages = f.workspace.metadata_status().unwrap().allocated_pages;
        for index in 0..expected {
            let name = long_name(index);
            assert_eq!(name.len(), 255);
            let a = mkdir(&f, f.workspace.root().serial, &name, 0o755, 0);
            accepted.push((name, a));
            f.workspace.forget(a.serial, 1, ReferenceScope::Local);
        }
        assert_eq!(reservations(&f), expected);
        let before_refusal = state(&f.workspace);
        assert_eq!(
            f.workspace.mkdir(
                f.workspace.root().serial,
                &long_name(expected),
                0o755,
                0,
                deadline()
            ),
            Err(WorkspaceError::Capacity)
        );
        assert_eq!(state(&f.workspace), before_refusal);
        assert_eq!(reservations(&f), expected);
        let entries = listing(&f.workspace, f.workspace.root().serial);
        for (name, attrs) in &accepted {
            assert_eq!(entries.get(name), Some(&attrs.serial));
            assert_eq!(
                lookup(&f.workspace, f.workspace.root().serial, name),
                *attrs
            );
            f.workspace.forget(attrs.serial, 1, ReferenceScope::Local);
        }
        assert!(!entries.contains_key(&long_name(expected)));
        let pages = f.workspace.metadata_status().unwrap().allocated_pages;
        assert!(pages > initial_pages + 1);
        assert_eq!(publications(&f), 0);
        let report = commit(&f);
        assert_eq!(publications(&f), 1);
        let observed = f.native.observations.lock().unwrap();
        let actual = observed
            .operations
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(actual.new_directories.len(), expected);
        assert_eq!(actual.inodes.len(), 1);
        assert_eq!(actual.directory_metadata.len(), 1);
        let actual_bytes = encode_request(&Request {
            id: 1,
            generation: 1,
            store: 1,
            profile: HISTORY_PROFILE,
            deadline_ms: 10000,
            response_bytes: 0,
            operation: Operation::HistoryCommand(HistoryCommand::Commit(actual)),
        })
        .unwrap()
        .len();
        assert_eq!(actual_bytes, wire_bytes);
        drop(observed);
        let saved = attr(f.native.attributes(root(&report), b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 0, 4), b"DIRS");
        for (name, attrs) in &accepted {
            saved_attributes(&f, root(&report), name, *attrs);
        }
        missing(&f, before.effective_root, &long_name(0));
        println!("MKDIR_CAPACITY accepted={expected} name_bytes=255 encoded_bytes={actual_bytes} next_bytes={} limit=32768 dirty={} dirty_limit=128 pages_before={initial_pages} pages_after={pages}", actual_bytes + 299, expected + 2);
        check("fixed-long-name-envelope-tree-pages-and-mixed-file-Commit");
        close(&f, &[data.serial]);
    }

    #[test]
    #[ignore = "requires real mounted refusal and native Service"]
    fn mkdir_refusals() {
        let f = Fixture::new(Gate::None);
        let parent = f.workspace.root().serial;
        let initial = state(&f.workspace);
        let before = reservations(&f);
        for name in [
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
                f.workspace.mkdir(parent, name, 0o755, 0, deadline()),
                Err(WorkspaceError::InvalidInput)
            );
        }
        for (mode, umask) in [(0o4755, 0), (0o755, 0o1000)] {
            assert_eq!(
                f.workspace.mkdir(parent, b"bad", mode, umask, deadline()),
                Err(WorkspaceError::InvalidInput)
            );
        }
        assert_eq!(
            f.workspace.mkdir(parent, b"late", 0o755, 0, Instant::now()),
            Err(WorkspaceError::Deadline)
        );
        let file = f.lookup(b"data.bin");
        assert_eq!(
            f.workspace
                .mkdir(file.serial, b"child", 0o755, 0, deadline()),
            Err(WorkspaceError::NotDirectory)
        );
        assert_eq!(state(&f.workspace), initial);
        assert_eq!(reservations(&f), before);
        let ro = f
            .host
            .attach(
                Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        assert_eq!(
            ro.mkdir(ro.root().serial, b"readonly", 0o755, 0, deadline()),
            Err(WorkspaceError::ReadOnly)
        );
        ro.close_clean().unwrap();
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        assert!(f.workspace.status().unwrap().mounted);
        assert!(matches!(
            f.workspace.mkdir(parent, b"mounted", 0o755, 0, deadline()),
            Err(WorkspaceError::Unsupported)
        ));
        mount.unmount(deadline()).unwrap();
        assert_eq!(state(&f.workspace), initial);
        assert_eq!(reservations(&f), before);
        let mut options = Fixture::options("owner", 33, WorkspaceAccess::LocalEdit);
        options.owner_uid = 1000;
        options.owner_gid = 1000;
        let owner_root =
            PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).join("owner-host");
        {
            use std::os::unix::fs::MetadataExt;
            for path in [&owner_root, &owner_root.join("workspace")] {
                let metadata = std::fs::symlink_metadata(path).unwrap();
                assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
                assert_eq!(
                    (metadata.uid(), metadata.gid(), metadata.mode() & 0o777),
                    (1000, 1000, 0o700)
                );
            }
        }
        let owner_host = WorkspaceHost::new(
            WorkspaceConfig {
                root: owner_root,
                max_count: 1,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            f.native.delivery(),
        )
        .unwrap();
        let owner = owner_host.attach(options, deadline()).unwrap();
        let denied = [
            owner
                .mkdir(owner.root().serial, b"no-write", 0o500, 0, deadline())
                .unwrap(),
            owner
                .mkdir(owner.root().serial, b"no-search", 0o600, 0, deadline())
                .unwrap(),
        ];
        let owner_state = state(&owner);
        let reserved = reservations(&f);
        for a in denied {
            assert_eq!(
                owner.mkdir(a.serial, b"child", 0o755, 0, deadline()),
                Err(WorkspaceError::Denied)
            );
        }
        assert_eq!(state(&owner), owner_state);
        assert_eq!(reservations(&f), reserved);
        owner.commit(deadline()).unwrap();
        for a in denied {
            owner.forget(a.serial, 1, ReferenceScope::Local);
        }
        owner.close_clean().unwrap();
        check("invalid-readonly-access-deadline-and-mounted-refuse-before-Reserve");
        close(&f, &[file.serial]);
    }

    fn reserve(native: &Native, scope: Root) -> u64 {
        let response = native
            .request(
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
                0,
                &mut std::io::sink(),
            )
            .unwrap();
        let Response::History(result) = response else {
            panic!("reservation")
        };
        let HistoryResult::Reservation {
            start,
            count: 1,
            scope: actual,
        } = *result
        else {
            panic!("reservation")
        };
        assert_eq!(actual, scope);
        start
    }
    fn reserve_failure(unknown: bool) {
        let native = Native::new(Gate::None);
        let failing = Arc::new(AtomicBool::new(true));
        let attempts = Arc::new(AtomicUsize::new(0));
        let deliver_native = native.clone();
        let fail = failing.clone();
        let count = attempts.clone();
        let delivery: OperationDelivery = Arc::new(move |request, input, output, end| {
            if fail.load(Ordering::Acquire)
                && matches!(
                    request.operation,
                    Operation::HistoryCommand(HistoryCommand::ReserveInodes { .. })
                )
            {
                count.fetch_add(1, Ordering::AcqRel);
                deliver_native
                    .observations
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
                        deliver_native.private,
                    )
                } else {
                    (
                        deliver_native.endpoint,
                        2,
                        pipe::key(&std::env::var("LAYERFS_RESERVE_PRIVATE_KEY").unwrap()).unwrap(),
                    )
                };
                let mut client = Client::new(connect_until(
                    endpoint,
                    selector,
                    &private,
                    &deliver_native.server,
                    end,
                )?)?;
                client.call_until(request, input, output, end)
            } else {
                deliver_native.call(request, input, output, end)
            }
        });
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()),
                max_count: 3,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            delivery,
        )
        .unwrap();
        let workspace = host
            .attach(
                Fixture::options("stage", 31, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        let f = Fixture {
            host,
            workspace,
            native,
        };
        let before = snapshot(&f);
        let marker = reserve(&f.native, before.scope);
        let initial = state(&f.workspace);
        let root_attr = f.workspace.getattr(f.workspace.root().serial).unwrap();
        let failure = f
            .workspace
            .mkdir(root_attr.serial, b"failed", 0o755, 0, deadline())
            .unwrap_err();
        assert!(
            matches!(&failure, WorkspaceError::Service(error) if error.unknown == unknown
            && (unknown || error.code == Code::Denied)),
            "{failure:?}"
        );
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        assert_eq!(state(&f.workspace), initial);
        assert_eq!(f.workspace.getattr(root_attr.serial).unwrap(), root_attr);
        assert_eq!(snapshot(&f), before);
        assert_eq!(publications(&f), 0);
        assert!(!listing(&f.workspace, root_attr.serial).contains_key(b"failed".as_slice()));
        let next = reserve(&f.native, before.scope);
        assert_eq!(next, marker + if unknown { 2 } else { 1 });
        failing.store(false, Ordering::Release);
        let after = mkdir(&f, root_attr.serial, b"after", 0o755, 0);
        assert_eq!(after.serial, next + 1);
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        let report = commit(&f);
        saved_attributes(&f, root(&report), b"after", after);
        missing(&f, root(&report), b"failed");
        println!("MKDIR_RESERVE unknown={unknown} attempts=1 before={marker} after={next} successful={} failure={failure:?}", after.serial);
        check(if unknown {
            "unknown-Reserve-consumed-once-no-replay-or-namespace-publication"
        } else {
            "denied-Reserve-consumed-none-no-replay-or-namespace-publication"
        });
        close(&f, &[after.serial]);
    }
    #[test]
    #[ignore = "requires separate actual Service denied principal"]
    fn mkdir_reserve_denied() {
        reserve_failure(false);
    }
    #[test]
    #[ignore = "requires actual native Reserve terminal loss"]
    fn mkdir_reserve_unknown() {
        reserve_failure(true);
    }
}
