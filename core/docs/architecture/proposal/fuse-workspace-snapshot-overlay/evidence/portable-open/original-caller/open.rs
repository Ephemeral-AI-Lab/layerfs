//! Portable open and atomic handle/truncate publication through production APIs.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        path::PathBuf,
        sync::{Arc, Barrier},
        time::{Duration, Instant},
    };
    fn options(access: FileAccess, append: bool, truncate: bool) -> FileOpenOptions {
        FileOpenOptions {
            access,
            append,
            truncate,
        }
    }
    fn check(id: &str) {
        println!("OPEN_CHECK {id} PASS");
    }
    fn observe(f: &Fixture) {
        println!(
            "COMMIT_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn root(report: &CommitReport) -> Root {
        match &report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            CommitOutcomeWire::UpToDate { root, .. } => *root,
        }
    }
    fn open(
        f: &Fixture,
        serial: u64,
        access: FileAccess,
        append: bool,
        truncate: bool,
    ) -> HandleId {
        f.workspace
            .open_file(
                serial,
                options(access, append, truncate),
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap()
    }
    fn denied_read(f: &Fixture, handle: HandleId) {
        for (offset, length) in [(0, 0), (0, 1), (u64::MAX, 0), (u64::MAX, 1)] {
            assert!(matches!(
                f.workspace.read(handle, offset, length, deadline()),
                Err(WorkspaceError::BadHandle)
            ));
        }
    }

    #[test]
    #[ignore = "requires open_route.py native permission fixture"]
    fn open_permissions() {
        let native = Native::new(Gate::None);
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
        let mut config = Fixture::options("stage", 31, WorkspaceAccess::LocalEdit);
        config.owner_uid = 1001;
        config.owner_gid = 1001;
        let workspace = host.attach(config, deadline()).unwrap();
        let f = Fixture {
            host,
            workspace,
            native,
        };
        let data = f.lookup(b"data.bin");
        assert_eq!(data.mode, 0o444);
        assert_eq!(data.uid, 1001);
        let before = f.workspace.status().unwrap();
        for access in [FileAccess::WriteOnly, FileAccess::ReadWrite] {
            for truncate in [false, true] {
                assert_eq!(
                    f.workspace.open_file(
                        data.serial,
                        options(access, false, truncate),
                        ReferenceScope::Local,
                        deadline()
                    ),
                    Err(WorkspaceError::Denied)
                );
            }
        }
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        assert_eq!(f.workspace.status().unwrap().handles, 0);
        let read = open(&f, data.serial, FileAccess::ReadOnly, false, false);
        assert_eq!(f.read(read, 0, 4), [0, 1, 2, 3]);
        f.workspace.release(read).unwrap();
        let other = f.lookup(b"other.bin");
        let writable = open(&f, other.serial, FileAccess::ReadWrite, true, false);
        assert_eq!(f.read(writable, 0, 4), [0, 1, 2, 3]);
        f.workspace.release(writable).unwrap();
        f.workspace.close_clean().unwrap();
        check("nonroot-owner-DAC-applies-to-open-access-and-truncation");
    }

    #[test]
    #[ignore = "requires open_route.py native fixture"]
    fn open_modes() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let legacy = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.read(legacy, 0, 4), [0, 1, 2, 3]);
        f.workspace.release(legacy).unwrap();
        for (access, append) in [
            (FileAccess::ReadOnly, false),
            (FileAccess::WriteOnly, false),
            (FileAccess::WriteOnly, true),
            (FileAccess::ReadWrite, false),
            (FileAccess::ReadWrite, true),
        ] {
            let handle = open(&f, data.serial, access, append, false);
            assert_eq!(f.workspace.handle_attributes(handle).unwrap(), data);
            f.workspace.flush(handle).unwrap();
            if access == FileAccess::WriteOnly {
                denied_read(&f, handle)
            } else {
                assert_eq!(f.read(handle, 0, 4), [0, 1, 2, 3]);
            }
            f.workspace.release(handle).unwrap();
            assert_eq!(f.workspace.flush(handle), Err(WorkspaceError::BadHandle));
        }
        let before = f.workspace.status().unwrap();
        for (append, truncate) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                f.workspace.open_file(
                    data.serial,
                    options(FileAccess::ReadOnly, append, truncate),
                    ReferenceScope::Local,
                    deadline()
                ),
                Err(WorkspaceError::InvalidInput)
            );
        }
        assert_eq!(
            f.workspace.open_file(
                data.serial,
                FileOpenOptions::default(),
                ReferenceScope::Projection,
                deadline()
            ),
            Err(WorkspaceError::Busy)
        );
        assert_eq!(
            f.workspace.open_file(
                f.workspace.root().serial,
                FileOpenOptions::default(),
                ReferenceScope::Local,
                deadline()
            ),
            Err(WorkspaceError::IsDirectory)
        );
        assert_eq!(
            f.workspace.open_file(
                u64::MAX,
                FileOpenOptions::default(),
                ReferenceScope::Local,
                deadline()
            ),
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(
            f.workspace.open_file(
                data.serial,
                options(FileAccess::WriteOnly, false, true),
                ReferenceScope::Local,
                Instant::now() - Duration::from_secs(1)
            ),
            Err(WorkspaceError::Deadline)
        );
        assert_eq!(f.workspace.status().unwrap().handles, 0);
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        let ro = f
            .host
            .attach(
                Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        let r = ro
            .lookup(
                ro.root().serial,
                b"data.bin",
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        assert_eq!(
            ro.open_file(
                r.serial,
                options(FileAccess::WriteOnly, false, false),
                ReferenceScope::Local,
                deadline()
            ),
            Err(WorkspaceError::ReadOnly)
        );
        let h = ro
            .open_file(
                r.serial,
                FileOpenOptions::default(),
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        ro.release(h).unwrap();
        ro.close_clean().unwrap();
        f.workspace.close_clean().unwrap();
        check("portable-open-rights-legacy-readonly-and-unsupported-options");
    }

    #[test]
    #[ignore = "requires full handle admission followed by actual truncate and Commit"]
    fn open_capacity() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        assert_eq!(f.lookup(b"alias").serial, data.serial);
        let mut handles = Vec::new();
        for _ in 0..128 {
            handles.push(
                f.workspace
                    .open(data.serial, ReferenceScope::Local)
                    .unwrap(),
            );
        }
        let before = f.workspace.status().unwrap();
        let backing = f.workspace.backing_status().unwrap();
        assert_eq!(
            f.workspace.open_file(
                data.serial,
                options(FileAccess::ReadWrite, false, true),
                ReferenceScope::Local,
                deadline()
            ),
            Err(WorkspaceError::Capacity)
        );
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (after.handles, after.revision, after.dirty_inodes),
            (128, before.revision, 0)
        );
        let next = f.workspace.backing_status().unwrap();
        assert_eq!(
            (next.allocated_bytes, next.reserved_bytes),
            (backing.allocated_bytes, backing.reserved_bytes)
        );
        assert_eq!(f.read(handles[0], 0, 4), [0, 1, 2, 3]);
        let freed = handles.pop().unwrap();
        f.workspace.release(freed).unwrap();
        let truncated = open(&f, data.serial, FileAccess::WriteOnly, false, true);
        assert!(truncated > freed);
        assert_eq!(f.workspace.handle_attributes(truncated).unwrap().size, 0);
        assert!(f.read(handles[0], 0, 1).is_empty());
        denied_read(&f, truncated);
        f.workspace.flush(truncated).unwrap();
        let updated = f.workspace.getattr(data.serial).unwrap();
        assert!(f.workspace.status().unwrap().revision > before.revision);
        let commit = f.workspace.commit(deadline()).unwrap();
        let saved = attr(f.native.attributes(root(&commit), b"data.bin"));
        assert_eq!(saved.2, 0);
        assert_eq!(
            (saved.3, saved.4),
            (updated.mtime_seconds, updated.mtime_nanoseconds)
        );
        assert_eq!(attr(f.native.attributes(root(&commit), b"alias")), saved);
        let before = f.workspace.status().unwrap().revision;
        f.workspace.release(truncated).unwrap();
        let again = open(&f, data.serial, FileAccess::ReadWrite, false, true);
        assert!(f.workspace.status().unwrap().revision > before);
        let count = f.native.observations.lock().unwrap().saved_files.len();
        f.workspace.commit(deadline()).unwrap();
        assert_eq!(
            f.native.observations.lock().unwrap().saved_files.len(),
            count
        );
        f.workspace.release(again).unwrap();
        for h in handles {
            f.workspace.release(h).unwrap();
        }
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("full-handle-table-never-truncates-and-successful-open-does");
    }

    #[test]
    #[ignore = "requires two real callers contending for the final handle slot"]
    fn open_last_slot() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let mut held = Vec::new();
        for _ in 0..127 {
            held.push(
                f.workspace
                    .open(data.serial, ReferenceScope::Local)
                    .unwrap(),
            );
        }
        let last = *held.last().unwrap();
        let start = Arc::new(Barrier::new(3));
        let mut pending = Vec::new();
        for _ in 0..2 {
            let ws = f.workspace.clone();
            let start = start.clone();
            pending.push(std::thread::spawn(move || {
                start.wait();
                ws.open_file(
                    data.serial,
                    options(FileAccess::ReadWrite, false, true),
                    ReferenceScope::Local,
                    deadline(),
                )
            }));
        }
        start.wait();
        let results: Vec<_> = pending.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == Err(WorkspaceError::Capacity))
                .count(),
            1
        );
        let winner = results.into_iter().find_map(Result::ok).unwrap();
        assert!(winner > last);
        assert_eq!(f.workspace.status().unwrap().handles, 128);
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, 0);
        f.workspace.release(winner).unwrap();
        let next = open(&f, data.serial, FileAccess::ReadOnly, false, false);
        assert!(next > winner);
        f.workspace.release(next).unwrap();
        for h in held {
            f.workspace.release(h).unwrap();
        }
        f.workspace.commit(deadline()).unwrap();
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("last-handle-slot-admits-one-truncating-open-without-id-reuse");
    }

    struct RestoreLimit;
    impl Drop for RestoreLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires native file-size failure during truncate preparation"]
    fn open_metadata_failure() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let prior = open(&f, data.serial, FileAccess::ReadOnly, false, false);
        f.workspace.release(prior).unwrap();
        let before = f.workspace.status().unwrap();
        let restore = RestoreLimit;
        file_limit("2048");
        let result = f.workspace.open_file(
            data.serial,
            options(FileAccess::ReadWrite, false, true),
            ReferenceScope::Local,
            deadline(),
        );
        drop(restore);
        assert!(matches!(result, Err(WorkspaceError::Backing(_))));
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (after.handles, after.revision, after.dirty_inodes),
            (0, before.revision, 0)
        );
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        let read = open(&f, data.serial, FileAccess::ReadOnly, false, false);
        assert!(read > prior + 1);
        assert_eq!(f.read(read, 0, 4), [0, 1, 2, 3]);
        f.workspace.release(read).unwrap();
        assert!(f.workspace.metadata_status().unwrap().admission_stopped);
        println!("COMMIT_FAILURE {result:?}");
        observe(&f);
        check("failed-truncating-open-releases-pending-handle-not-backing-custody");
    }

    fn stale() -> (Fixture, NodeAttributes) {
        let f = Fixture::new(Gate::AttributesAfterCommit);
        let data = f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        f.workspace.commit(deadline()).unwrap();
        (f, data)
    }
    #[test]
    #[ignore = "requires real stale-canonical refresh held after pending handle reservation"]
    fn open_forget() {
        let (f, data) = stale();
        let prior = open(&f, data.serial, FileAccess::ReadOnly, false, false);
        f.workspace.release(prior).unwrap();
        let pending_id = prior + 1;
        let ws = f.workspace.clone();
        let pending = std::thread::spawn(move || {
            ws.open_file(
                data.serial,
                options(FileAccess::ReadWrite, false, true),
                ReferenceScope::Local,
                deadline(),
            )
        });
        f.native.wait_attributes();
        assert_eq!(f.workspace.status().unwrap().handles, 1);
        assert_eq!(
            f.workspace.handle_attributes(pending_id),
            Err(WorkspaceError::BadHandle)
        );
        assert_eq!(
            f.workspace.flush(pending_id),
            Err(WorkspaceError::BadHandle)
        );
        assert_eq!(
            f.workspace.release(pending_id),
            Err(WorkspaceError::BadHandle)
        );
        assert!(matches!(
            f.workspace.read(pending_id, 0, 0, deadline()),
            Err(WorkspaceError::BadHandle)
        ));
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, data.size);
        f.native.release_attributes();
        let handle = pending.join().unwrap().unwrap();
        assert_eq!(handle, pending_id);
        assert_eq!(f.workspace.handle_attributes(handle).unwrap().size, 0);
        f.workspace.commit(deadline()).unwrap();
        f.workspace.release(handle).unwrap();
        assert_eq!(
            f.workspace.getattr(data.serial),
            Err(WorkspaceError::NotFound)
        );
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("pending-truncating-open-pins-node-across-forget-and-Commit");
    }

    #[test]
    #[ignore = "requires real deadline expiry before stale refresh completes"]
    fn open_deadline() {
        let (f, data) = stale();
        let before = f.workspace.getattr(data.serial).unwrap();
        let status = f.workspace.status().unwrap();
        let end = Instant::now() + Duration::from_millis(200);
        let ws = f.workspace.clone();
        let pending = std::thread::spawn(move || {
            ws.open_file(
                data.serial,
                options(FileAccess::ReadWrite, false, true),
                ReferenceScope::Local,
                end,
            )
        });
        f.native.wait_attributes();
        assert_eq!(f.workspace.status().unwrap().handles, 1);
        while Instant::now() <= end {
            std::thread::sleep(Duration::from_millis(10));
        }
        f.native.release_attributes();
        let result = pending.join().unwrap();
        assert!(result.is_err());
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), before);
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (after.handles, after.revision, after.dirty_inodes),
            (0, status.revision, 0)
        );
        println!("COMMIT_FAILURE {result:?}");
        f.workspace.close_clean().unwrap();
        check("expired-truncate-preparation-publishes-neither-handle-nor-length");
    }

    #[test]
    #[ignore = "requires captured G delivery held while truncating open publishes D1"]
    fn open_successor() {
        let f = Fixture::new(Gate::Delivery);
        let data = f.lookup(b"data.bin");
        let old = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        f.native.wait_entered();
        let truncated = open(&f, data.serial, FileAccess::ReadWrite, true, true);
        assert_eq!(f.workspace.handle_attributes(old).unwrap().size, 0);
        assert!(f.read(old, 0, 1).is_empty());
        f.native.release();
        let first = saving.join().unwrap().unwrap();
        let saved = attr(f.native.attributes(root(&first), b"data.bin"));
        assert_eq!(saved.2, data.size);
        assert_eq!(f.native.bytes(saved.1, 10, 4), b"GGGG");
        assert_eq!(f.workspace.handle_attributes(truncated).unwrap().size, 0);
        let second = f.workspace.commit(deadline()).unwrap();
        assert_eq!(attr(f.native.attributes(root(&second), b"data.bin")).2, 0);
        f.workspace.release(old).unwrap();
        f.workspace.release(truncated).unwrap();
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("truncating-open-during-save-keeps-G-and-commits-D1-empty-file");
    }
}
