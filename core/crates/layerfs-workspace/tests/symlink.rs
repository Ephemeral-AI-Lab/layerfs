//! Native atomic symlink publication and explicit captured-generation saves.
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
        println!("SYMLINK_CHECK {id} PASS");
    }
    fn fixture_with(native: Arc<Native>, delivery: OperationDelivery) -> Fixture {
        let root =
            PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).join("symlink-owner");
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
        println!("MKDIR_RESOURCE symlink_owner_uid=1000 symlink_owner_gid=1000 actual_process_uid={} scope=native-configured-owner", nix::unistd::geteuid());
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
                "SYMLINK_DIAGNOSTIC commit-start remaining_us={}",
                end.saturating_duration_since(Instant::now()).as_micros()
            );
            let result = f.workspace.commit(end);
            eprintln!(
                "SYMLINK_DIAGNOSTIC commit-end elapsed_us={} remaining_us={} result={result:?}",
                started.elapsed().as_micros(),
                end.saturating_duration_since(Instant::now()).as_micros()
            );
            if result.is_err() {
                eprintln!(
                    "SYMLINK_DIAGNOSTIC failed-commit-public-status={:?}",
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
    fn symlink(f: &Fixture, name: &[u8], target: &[u8]) -> NodeAttributes {
        let handles = f.workspace.status().unwrap().handles;
        let a = f
            .workspace
            .symlink(f.workspace.root().serial, name, target, deadline())
            .unwrap();
        assert_eq!(
            (a.kind, a.size, a.mode, a.references, a.uid, a.gid),
            (NodeKind::Symlink, target.len() as u64, 0o777, 1, 1000, 1000)
        );
        assert_eq!(f.workspace.status().unwrap().handles, handles);
        assert_eq!(f.workspace.getattr(a.serial).unwrap(), a);
        a
    }
    fn readlink(f: &Fixture, serial: u64) -> Vec<u8> {
        f.workspace
            .readlink(serial, deadline())
            .unwrap()
            .as_ref()
            .to_vec()
    }
    fn saved(f: &Fixture, root: Root, name: &[u8], target: &[u8], a: NodeAttributes) {
        let Response::Attributes {
            serial,
            kind,
            references,
            size,
            mode,
            mtime,
            nanoseconds,
            ..
        } = f.native.attributes(root, name)
        else {
            panic!("attributes");
        };
        assert_eq!(
            (serial, kind, references, size, mode, mtime, nanoseconds),
            (
                a.serial,
                3,
                1,
                target.len() as u64,
                0o777,
                a.mtime_seconds,
                a.mtime_nanoseconds
            )
        );
        assert_eq!(
            f.native
                .request(
                    Operation::Inspect {
                        root,
                        query: Inspect::Readlink {
                            path: name.to_vec()
                        }
                    },
                    0,
                    &mut std::io::sink()
                )
                .unwrap(),
            Response::Link(target.to_vec())
        );
    }
    fn constructors(f: &Fixture) -> usize {
        count(f, |op| matches!(op, Operation::ConstructSymlink { .. }))
    }
    #[test]
    #[ignore = "requires actual Service and native fresh target custody"]
    fn symlink_semantics() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let parent = f.workspace.root().serial;
        let old = f.workspace.opendir(parent, ReferenceScope::Local).unwrap();
        let page = f.workspace.readdir(old, 0, 2, deadline()).unwrap();
        let cookie = page.entries().last().unwrap().cookie;
        drop(page);
        let old_tail = tail(&f.workspace, old, cookie);
        assert_eq!(old_tail.len(), 3);
        let payloads = f.workspace.backing_status().unwrap().payloads;
        let targets = [
            (b"empty".as_slice(), b"".as_slice()),
            (b"dangling".as_slice(), b"../missing".as_slice()),
            (b"opaque".as_slice(), b"\xff/\x80".as_slice()),
            (b"self".as_slice(), b"self".as_slice()),
        ];
        let mut created = Vec::new();
        for (name, target) in targets {
            let a = symlink(&f, name, target);
            if target.is_empty() {
                assert_eq!(f.workspace.backing_status().unwrap().payloads, payloads);
            }
            let calls = f.native.observations.lock().unwrap().operations.len();
            assert_eq!(readlink(&f, a.serial), target);
            assert_eq!(
                f.native.observations.lock().unwrap().operations.len(),
                calls,
                "fresh readlink must stay local"
            );
            created.push((name, target, a));
        }
        let live = listing(&f.workspace, parent);
        for (name, target, a) in &created {
            assert_eq!(live.get(*name), Some(&a.serial));
            f.workspace.forget(a.serial, 1, ReferenceScope::Local);
            assert_eq!(
                f.workspace.getattr(a.serial),
                Err(WorkspaceError::NotFound),
                "readdir leaked a lookup ref"
            );
            assert_eq!(lookup(&f.workspace, parent, name), *a);
            assert_eq!(readlink(&f, a.serial), *target);
        }
        assert_eq!(tail(&f.workspace, old, cookie), old_tail);
        assert_eq!(reserves(&f), 4);
        assert_eq!(constructors(&f), 0);
        assert_eq!(publications(&f), 0);
        assert_eq!(snapshot(&f), before);
        let report = commit(&f);
        assert_eq!(publications(&f), 1);
        assert_eq!(constructors(&f), 4);
        for (name, target, a) in &created {
            saved(&f, root(&report), name, target, *a);
            missing(&f, before.effective_root, name);
            assert_eq!(readlink(&f, a.serial), *target);
        }
        assert_eq!(tail(&f.workspace, old, cookie), old_tail);
        let observed = f.native.observations.lock().unwrap();
        let p = observed
            .operations
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            p.new_symlink_serials,
            created.iter().map(|(_, _, a)| a.serial).collect::<Vec<_>>()
        );
        assert!(p.new_file_serials.is_empty());
        assert!(p.inodes.iter().all(|inode| inode.kind == 3));
        drop(observed);
        f.workspace.releasedir(old).unwrap();
        check("native-opaque-empty-self-symlinks-forget-listing-and-explicit-Commit");
        close(
            &f,
            &[],
            &created
                .iter()
                .map(|(_, _, a)| (a.serial, 1))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    #[ignore = "requires first symlink constructor gate and actual C5 reconciliation"]
    fn symlink_successor() {
        let f = fixture(Gate::Delivery);
        let before = snapshot(&f);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                data.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    append: false,
                    truncate: false,
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let a = symlink(&f, b"captured", b"../captured/\xff");
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        let calls = f.native.observations.lock().unwrap().operations.len();
        assert_eq!(readlink(&f, a.serial), b"../captured/\xff");
        f.workspace
            .write_file(handle, 0, &f.own(b"EDIT"), deadline())
            .unwrap();
        assert_eq!(f.read(handle, 0, 4), b"EDIT");
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            calls,
            "captured readlink/local D1 write opened a remote lane"
        );
        f.native.release();
        let stage = saving.join().unwrap().unwrap();
        let frozen = stage.stage().clone();
        saved(
            &f,
            frozen.candidate_root,
            b"captured",
            b"../captured/\xff",
            a,
        );
        let old = attr(f.native.attributes(frozen.candidate_root, b"data.bin"));
        assert_eq!(f.native.bytes(old.1, 0, 4), vec![0, 1, 2, 3]);
        let born = symlink(&f, b"born", b"born");
        let edited = f.workspace.getattr(data.serial).unwrap();
        let one = f.workspace.commit_staged(&stage, deadline()).unwrap();
        assert_eq!(root(&one), frozen.candidate_root);
        assert_eq!(readlink(&f, a.serial), b"../captured/\xff");
        assert_eq!(readlink(&f, born.serial), b"born");
        assert_eq!(f.read(handle, 0, 4), b"EDIT");
        missing(&f, root(&one), b"born");
        let start = f.native.observations.lock().unwrap().operations.len();
        let two = commit(&f);
        saved(&f, root(&two), b"captured", b"../captured/\xff", a);
        saved(&f, root(&two), b"born", b"born", born);
        let value = f.native.attributes(root(&two), b"data.bin");
        assert!(
            matches!(&value,Response::Attributes{serial,mode,mtime,nanoseconds,..}if *serial==edited.serial&&*mode==edited.mode&&*mtime==edited.mtime_seconds&&*nanoseconds==edited.mtime_nanoseconds)
        );
        let canonical = attr(value);
        assert_eq!(canonical.2, 64 * 1024 * 1024);
        assert_eq!(f.native.bytes(canonical.1, 0, 4), b"EDIT");
        let observed = f.native.observations.lock().unwrap();
        let targets: Vec<_> = observed.operations[start..]
            .iter()
            .filter_map(|op| match op {
                Operation::ConstructSymlink { target } => Some(target.as_slice()),
                _ => None,
            })
            .collect();
        assert_eq!(targets, vec![b"born".as_slice()]);
        assert!(observed.operations[start..]
            .iter()
            .any(|op| matches!(op,Operation::EditFile{root,..}if *root==old.1)));
        let p = observed.operations[start..]
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p),
                _ => None,
            })
            .unwrap();
        assert_eq!(p.new_symlink_serials, vec![born.serial]);
        assert!(p.new_file_serials.is_empty());
        assert_eq!(p.base, frozen.candidate_root);
        drop(observed);
        assert_eq!(reserves(&f), 2);
        assert_eq!(publications(&f), 2);
        missing(&f, before.effective_root, b"captured");
        missing(&f, before.effective_root, b"born");
        drop(stage);
        check("captured-local-readlink-and-D1-file-plus-symlink-reconcile");
        close(
            &f,
            &[handle],
            &[(data.serial, 1), (a.serial, 1), (born.serial, 1)],
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
            kind: 3,
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
            new_file_serials: Vec::new(),
            new_symlink_serials: (0..count).map(|i| 1000 + i as u64).collect(),
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
    #[ignore = "requires fixed 93 symlinks and complete exact-v3-frontier Commit"]
    fn symlink_capacity() {
        const ACCEPTED: usize = 93;
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
            panic!("old file");
        };
        let existing = InodeChange {
            serial,
            kind,
            content,
            metadata,
        };
        let bytes = encoded(&before, existing, ACCEPTED).unwrap().len();
        assert_eq!(bytes, 228 + 34 + 73 + 9 + ACCEPTED * (73 + 265 + 8));
        assert_eq!(bytes, 32522);
        assert_eq!(bytes + 346, 32868);
        assert_eq!(
            encoded(&before, existing, ACCEPTED + 1).unwrap_err().code,
            Code::Capacity
        );
        let initial = status(&f);
        let parent = f.workspace.root().serial;
        assert_eq!(
            f.workspace
                .symlink(parent, b"over-target", &[b'x'; 4097], deadline()),
            Err(WorkspaceError::Capacity)
        );
        assert_eq!(
            f.workspace
                .symlink(parent, b"nul-target", b"a\0b", deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(status(&f), initial);
        assert_eq!(reserves(&f), 0);
        f.edit(b"data.bin", 0, 4, b"EDIT");
        let edited = f.workspace.getattr(data.serial).unwrap();
        let maximum = vec![b'x'; 4096];
        let mut accepted = Vec::new();
        for index in 0..ACCEPTED {
            let target = if index == 0 { maximum.as_slice() } else { b"" };
            let a = symlink(&f, &name(index), target);
            assert_eq!(readlink(&f, a.serial), target);
            accepted.push(a);
            f.workspace.forget(a.serial, 1, ReferenceScope::Local);
            assert_eq!(f.workspace.getattr(a.serial), Err(WorkspaceError::NotFound));
        }
        let full = status(&f);
        assert_eq!(reserves(&f), ACCEPTED);
        assert_eq!(
            f.workspace
                .symlink(parent, &name(ACCEPTED), b"", deadline()),
            Err(WorkspaceError::Capacity)
        );
        assert_eq!(status(&f), full);
        assert_eq!(reserves(&f), ACCEPTED);
        assert_eq!(constructors(&f), 0);
        assert_eq!(publications(&f), 0);
        let report = commit(&f);
        assert_eq!(publications(&f), 1);
        assert_eq!(constructors(&f), ACCEPTED);
        let observed = f.native.observations.lock().unwrap();
        let prepared = observed
            .operations
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(prepared.inodes.len(), ACCEPTED + 1);
        assert_eq!(prepared.new_symlink_serials.len(), ACCEPTED);
        assert!(prepared.new_file_serials.is_empty());
        assert_eq!(prepared.directory_metadata.len(), 1);
        assert_eq!(encode_prepared(prepared).unwrap().len(), bytes);
        let lengths: Vec<_> = observed
            .operations
            .iter()
            .filter_map(|op| match op {
                Operation::ConstructSymlink { target } => Some(target.len()),
                _ => None,
            })
            .collect();
        assert_eq!(lengths.len(), ACCEPTED);
        assert_eq!(lengths[0], 4096);
        assert!(lengths[1..].iter().all(|n| *n == 0));
        drop(observed);
        for (index, a) in accepted.iter().enumerate() {
            saved(
                &f,
                root(&report),
                &name(index),
                if index == 0 { &maximum } else { b"" },
                *a,
            );
        }
        let value = f.native.attributes(root(&report), b"data.bin");
        assert!(
            matches!(&value,Response::Attributes{serial,mode,mtime,nanoseconds,..}if *serial==edited.serial&&*mode==edited.mode&&*mtime==edited.mtime_seconds&&*nanoseconds==edited.mtime_nanoseconds)
        );
        let old = attr(value);
        assert_eq!(old.2, 64 * 1024 * 1024);
        assert_eq!(f.native.bytes(old.1, 0, 4), b"EDIT");
        println!("MKDIR_CAPACITY symlink_accepted=93 name_bytes=255 target_first=4096 target_rest=0 encoded_bytes=32522 next_bytes=32868 formula=228+34+73+9+93*(73+265+8) limit=32768 old43_file_capacity=NOT_QUALIFIED");
        check("exact-long-name-v3-frontier-full4096-target-and-mixed-file-Commit");
        close(&f, &[], &[(data.serial, 1)]);
    }

    #[test]
    #[ignore = "requires configured UID1000 access and unbound projection refusal"]
    fn symlink_refusals() {
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
                f.workspace.symlink(parent, bad, b"target", deadline()),
                Err(WorkspaceError::InvalidInput)
            );
        }
        assert_eq!(
            f.workspace.symlink(parent, b"nul", b"a\0b", deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(
            f.workspace
                .symlink(parent, b"long", &[b'x'; 4097], deadline()),
            Err(WorkspaceError::Capacity)
        );
        assert_eq!(
            f.workspace
                .symlink(parent, b"late", b"target", Instant::now()),
            Err(WorkspaceError::Deadline)
        );
        assert_eq!(status(&f), initial);
        assert_eq!(reserves(&f), 0);
        let data = f.lookup(b"data.bin");
        let denied = f.lookup(b"no-write");
        let no_search = f.lookup(b"no-search");
        for a in [denied, no_search] {
            assert_eq!(
                f.workspace
                    .symlink(a.serial, b"child", b"target", deadline()),
                Err(WorkspaceError::Denied)
            );
        }
        assert_eq!(
            f.workspace
                .symlink(data.serial, b"child", b"target", deadline()),
            Err(WorkspaceError::NotDirectory)
        );
        for name in [b"data.bin".as_slice(), b"no-write"] {
            assert_eq!(
                f.workspace.symlink(parent, name, b"target", deadline()),
                Err(WorkspaceError::Exists)
            );
        }
        assert!(matches!(
            f.workspace.readlink(data.serial, deadline()),
            Err(WorkspaceError::WrongKind)
        ));
        let mut options = Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly);
        options.owner_uid = 1000;
        options.owner_gid = 1000;
        let ro = f.host.attach(options, deadline()).unwrap();
        assert_eq!(
            ro.symlink(ro.root().serial, b"no", b"target", deadline()),
            Err(WorkspaceError::ReadOnly)
        );
        ro.close_clean().unwrap();
        let projected = Fixture::new(Gate::None);
        assert_eq!(projected.workspace.root().uid, 0);
        let mut mount = projected.workspace.reserve_mount().unwrap();
        assert!(projected.workspace.status().unwrap().mounted);
        assert_eq!(
            projected.workspace.symlink(
                projected.workspace.root().serial,
                b"mounted",
                b"target",
                deadline()
            ),
            Err(WorkspaceError::Busy)
        );
        assert_eq!(reserves(&projected), 0);
        assert_eq!(constructors(&projected), 0);
        assert_eq!(projected.workspace.backing_status().unwrap().payloads, 0);
        mount.finish().unwrap();
        projected.workspace.close_clean().unwrap();
        assert_eq!(reserves(&f), 0);
        assert_eq!(constructors(&f), 0);
        assert_eq!(publications(&f), 0);
        let a = symlink(&f, b"link", b"data.bin");
        let before = status(&f);
        assert_eq!(
            f.workspace.symlink(parent, b"link", b"other", deadline()),
            Err(WorkspaceError::Exists)
        );
        assert_eq!(
            f.workspace
                .symlink(a.serial, b"child", b"target", deadline()),
            Err(WorkspaceError::NotDirectory)
        );
        assert_eq!(
            f.workspace.open_file(
                a.serial,
                FileOpenOptions {
                    access: FileAccess::ReadOnly,
                    append: false,
                    truncate: false
                },
                ReferenceScope::Local,
                deadline()
            ),
            Err(WorkspaceError::WrongKind)
        );
        assert_eq!(
            f.workspace.set_len(a.serial, 0, deadline()),
            Err(WorkspaceError::WrongKind)
        );
        let edit = RangeEdit {
            start: 0,
            end: 0,
            replacement: f.own(b"x"),
        };
        assert_eq!(
            f.workspace
                .edit_file_range(&WorkspacePath::new(b"link").unwrap(), &edit, deadline()),
            Err(WorkspaceError::WrongKind)
        );
        drop(edit);
        assert_eq!(status(&f), before);
        assert_eq!(reserves(&f), 1);
        assert_eq!(readlink(&f, a.serial), b"data.bin");
        let report = commit(&f);
        saved(&f, root(&report), b"link", b"data.bin", a);
        check("native-symlink-target-name-access-kind-readonly-deadline-and-unbound-projection-refusals");
        close(
            &f,
            &[],
            &[
                (data.serial, 1),
                (denied.serial, 1),
                (no_search.serial, 1),
                (a.serial, 1),
            ],
        );
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
    #[ignore = "requires owned payload header fault; failed owner uses external-only teardown"]
    fn symlink_backing_failure() {
        use std::os::unix::fs::FileExt;
        let f = fixture(Gate::None);
        let branch = snapshot(&f);
        let target = b"fault-target";
        let a = symlink(&f, b"retained", target);
        let directory = PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap())
            .join("symlink-owner/private-backing/stage");
        let files: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("p-")
            })
            .collect();
        assert_eq!(files.len(), 1);
        let path = &files[0];
        let name = path.file_name().unwrap().to_str().unwrap();
        let parts: Vec<_> = name.split('-').collect();
        assert_eq!(parts.len(), 3);
        let payload = u64::from_str_radix(parts[1], 16).unwrap();
        assert_eq!(parts[2], "00000000");
        let identity = fs::symlink_metadata(path).unwrap();
        assert!(identity.is_file() && !identity.file_type().is_symlink());
        assert_eq!((identity.len(), identity.blocks() * 512), (8192, 8192));
        let raw = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let actual = raw.metadata().unwrap();
        assert_eq!(
            (actual.dev(), actual.ino()),
            (identity.dev(), identity.ino())
        );
        let mut original = [0];
        raw.read_exact_at(&mut original, 0).unwrap();
        raw.write_all_at(&[original[0] ^ 0xff], 0).unwrap();
        let calls = f.native.observations.lock().unwrap().operations.len();
        // Stage is the first corrupted-payload read. No prior readlink can
        // quarantine this owner before the symlink_target Source conversion.
        let result = f.workspace.stage(deadline());
        raw.write_all_at(&original, 0).unwrap();
        let restored = raw.metadata().unwrap();
        assert_eq!(
            (restored.dev(), restored.ino()),
            (identity.dev(), identity.ino())
        );
        drop(raw);
        let WorkspaceError::Stage(failure) = result.unwrap_err() else {
            panic!("typed Stage failure");
        };
        assert_eq!(failure.phase, StagePhase::FileSave);
        assert_eq!(failure.disposition, StageFailureDisposition::Unknown);
        let WorkspaceError::Backing(backing) = &failure.cause else {
            panic!("lost embedded BackingFailure: {failure:?}");
        };
        assert_eq!(backing.phase, BackingPhase::Read);
        assert_eq!(backing.kind, std::io::ErrorKind::InvalidData);
        assert_eq!(backing.payload, payload);
        assert_eq!(
            (backing.declared_bytes, backing.completed_bytes),
            (target.len() as u64, target.len() as u64)
        );
        assert_eq!(
            (
                backing.created_segments,
                backing.allocated_bytes,
                backing.reserved_bytes
            ),
            (1, 8192, 0)
        );
        assert!(!backing.accounting_complete && !backing.cleanup_failed);
        assert_eq!(
            failure.source_failure,
            Some(WorkspaceError::Backing(backing.clone()))
        );
        assert!(
            failure.known_stage.is_none()
                && failure.observed_stage.is_none()
                && failure.pending.is_none()
        );
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            calls,
            "failed materialization sent an RPC"
        );
        assert_eq!(constructors(&f), 0);
        assert_eq!(
            count(&f, |op| matches!(
                op,
                Operation::HistoryCommand(
                    HistoryCommand::StageChanges(_)
                        | HistoryCommand::Commit(_)
                        | HistoryCommand::CommitStaged { .. }
                )
            )),
            0
        );
        let status = f.workspace.status().unwrap();
        let submission = status.submission.unwrap();
        assert_eq!((submission.saved_files, submission.saved_metadata), (0, 0));
        assert_eq!(submission.phase, StagePhase::Failed);
        assert_eq!(submission.failure_phase, Some(StagePhase::FileSave));
        assert_eq!(submission.failure, Some(StageFailureDisposition::Unknown));
        assert_eq!(submission.inode, Some(a.serial));
        assert_eq!(submission.stage_token, None);
        let retained = f.workspace.backing_status().unwrap();
        assert_eq!((retained.payloads, retained.failed_payloads), (1, 1));
        assert!(retained.retained_payloads >= 1 && retained.allocated_bytes >= 8192);
        assert!(!retained.accounting_complete && retained.admission_stopped);
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        assert_eq!(snapshot(&f), branch);
        assert!(path.exists());
        println!("STAGE_FAILURE symlink_payload={payload} device={} inode={} restored_header=true cleanup=EXTERNAL_ONLY {failure:?}", identity.dev(), identity.ino());
        println!("STAGE_RESOURCE {status:?} {retained:?}");
        // Restoring the byte does not clear Unknown or release the failed G.
        // The separate stage driver tears down this runtime; no clean-close marker.
        check("typed-symlink-backing-read-preserves-Unknown-and-retained-custody");
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
        let initial_payloads = f.workspace.backing_status().unwrap().payloads;
        let parent = f.workspace.getattr(f.workspace.root().serial).unwrap();
        let error = f
            .workspace
            .symlink(parent.serial, b"failed", b"target", deadline())
            .unwrap_err();
        assert!(
            matches!(&error, WorkspaceError::Service(failure) if failure.unknown == unknown && (unknown || failure.code == Code::Denied)),
            "{error:?}"
        );
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        assert_eq!(status(&f), initial);
        assert_eq!(
            f.workspace.backing_status().unwrap().payloads,
            initial_payloads,
            "Reserve failed before target payload acquisition"
        );
        assert_eq!(constructors(&f), 0);
        assert_eq!(publications(&f), 0);
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
        let after = symlink(&f, b"after", b"after");
        assert_eq!(after.serial, next + 1);
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        let report = commit(&f);
        saved(&f, root(&report), b"after", b"after", after);
        missing(&f, root(&report), b"failed");
        println!("MKDIR_RESERVE symlink_unknown={unknown} attempts=1 before={marker} next={next} successful={} {error:?}", after.serial);
        check(if unknown {
            "symlink-unknown-Reserve-consumed-once-no-name-or-replay"
        } else {
            "symlink-denied-Reserve-no-name-handle-or-replay"
        });
        close(&f, &[], &[(after.serial, 1)]);
    }
    #[test]
    #[ignore = "requires actual Service denied principal"]
    fn symlink_reserve_denied() {
        reserve_failure(false);
    }
    #[test]
    #[ignore = "requires actual encrypted Reserve terminal withholding"]
    fn symlink_reserve_unknown() {
        reserve_failure(true);
    }
}
