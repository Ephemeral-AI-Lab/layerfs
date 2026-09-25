//! Platform-neutral projected range admission and publication through public APIs.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;

#[cfg(target_os = "linux")]
mod linux {
    use super::support::{deadline, Fixture, Gate, Native};
    use layerfs_workspace::{
        CoherenceStatus, FileAccess, FileOpenOptions, PortableAttributes, RangeEdit, RangePart,
        ReferenceScope, WorkspaceAccess, WorkspaceConfig, WorkspaceError, WorkspaceHost,
        DEFAULT_MEMORY_BUDGET_BYTES,
    };
    use std::{fs, io, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc};

    fn open(f: &Fixture, serial: u64, access: FileAccess, append: bool) -> u64 {
        f.workspace
            .open_file(
                serial,
                FileOpenOptions {
                    access,
                    append,
                    truncate: false,
                },
                ReferenceScope::Projection,
                deadline(),
            )
            .unwrap()
    }

    fn nonroot_fixture() -> Fixture {
        let native = Native::new(Gate::None);
        let root = PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap())
            .join("projected-range-owner");
        for path in [&root, &root.join("workspace")] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            std::os::unix::fs::chown(path, Some(1000), Some(1000)).unwrap();
        }
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root,
                max_count: 3,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            native.delivery(),
        )
        .unwrap();
        let mut options = Fixture::options("stage", 31, WorkspaceAccess::LocalEdit);
        options.owner_uid = 1000;
        options.owner_gid = 1000;
        let workspace = host.attach(options, deadline()).unwrap();
        println!(
            "PROJECTED_RANGE_OWNER configured_uid=1000 process_uid={} scope=native-configured-owner",
            nix::unistd::geteuid()
        );
        Fixture {
            host,
            workspace,
            native,
        }
    }

    #[test]
    #[ignore = "requires native service and private Workspace backing"]
    fn projected_range_semantics() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        f.workspace.set_len(file.serial, 8192, deadline()).unwrap();
        let mut mount = f.workspace.reserve_mount().unwrap();
        mount.bind_invalidation(Arc::new(|_, _, _| Ok(()))).unwrap();
        let handle = open(&f, file.serial, FileAccess::ReadWrite, false);
        let read_only = open(&f, file.serial, FileAccess::ReadOnly, false);
        let append = open(&f, file.serial, FileAccess::ReadWrite, true);
        let before = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        assert!(before.writable);
        assert_eq!(before.length, 8192);
        assert_eq!(before.stamp.inode, file.serial);
        assert!(
            !f.workspace
                .projected_range_state(read_only, file.serial)
                .unwrap()
                .writable
        );
        assert!(
            !f.workspace
                .projected_range_state(append, file.serial)
                .unwrap()
                .writable
        );
        assert_eq!(
            f.workspace.projected_range_state(handle, file.serial + 1),
            Err(WorkspaceError::BadHandle)
        );
        let baseline = f.read(handle, 0, 8192);
        let insert = RangeEdit {
            start: 4093,
            end: 4093,
            replacement: f.own(b"ABCD"),
        };
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        let receipt = permit
            .edit_file_range(handle, before.stamp, &insert, deadline())
            .unwrap();
        drop(permit);
        assert_eq!(receipt.accepted_bytes, 4);
        let after = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        assert_eq!(after.length, 8196);
        assert_eq!(after.stamp.revision, before.stamp.revision + 1);
        let mut expected = baseline.clone();
        expected.splice(4093..4093, *b"ABCD");
        assert_eq!(f.read(handle, 0, 8196), expected);
        assert_eq!(
            f.workspace.status().unwrap().range_accepted_payload_bytes,
            4
        );
        assert_eq!(f.workspace.status().unwrap().range_shifted_suffix_bytes, 0);

        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit.edit_file_range(handle, before.stamp, &insert, deadline()),
            Err(WorkspaceError::StaleStamp)
        );
        drop(permit);
        let empty = RangeEdit {
            start: 4093,
            end: 4093,
            replacement: f.own(b""),
        };
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit.edit_file_range(handle, after.stamp, &empty, deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        drop(permit);
        assert_eq!(
            f.workspace
                .projected_range_state(handle, file.serial)
                .unwrap(),
            after
        );
        assert_eq!(
            f.workspace.status().unwrap().range_accepted_payload_bytes,
            4
        );

        let delete = RangeEdit {
            start: 4093,
            end: 4097,
            replacement: f.own(b""),
        };
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        let deleted = permit
            .edit_file_range(handle, after.stamp, &delete, deadline())
            .unwrap();
        drop(permit);
        assert_eq!(deleted.accepted_bytes, 0);
        assert_eq!(f.read(handle, 0, 8192), baseline);
        assert_eq!(
            f.workspace.status().unwrap().range_accepted_payload_bytes,
            4
        );
        for id in [handle, read_only, append] {
            f.workspace.release(id).unwrap();
        }
        mount.finish().unwrap();
        f.workspace.commit(deadline()).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        drop((insert, empty, delete));
        f.workspace.close_clean().unwrap();
        println!("PROJECTED_RANGE_CHECK semantics PASS");
    }

    #[test]
    #[ignore = "requires native service and private Workspace backing"]
    fn projected_range_stream() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        f.workspace.set_len(file.serial, 8192, deadline()).unwrap();
        let mut mount = f.workspace.reserve_mount().unwrap();
        mount.bind_invalidation(Arc::new(|_, _, _| Ok(()))).unwrap();
        let handle = open(&f, file.serial, FileAccess::ReadWrite, false);
        let original = f.read(handle, 0, 8192);

        let bytes = vec![0x5a; 65536];
        let edit = RangeEdit {
            start: 4096,
            end: 4096,
            replacement: f.own(&bytes),
        };
        let before = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit
                .edit_file_range_stream(
                    handle,
                    before.stamp,
                    &edit,
                    &[RangePart::Bytes(65536)],
                    deadline()
                )
                .unwrap()
                .accepted_bytes,
            65536
        );
        drop(permit);
        let after = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        assert_eq!(after.stamp.revision, before.stamp.revision + 1);
        assert_eq!(after.length, 8192 + 65536);
        assert_eq!(f.read(handle, 4096, 65536), bytes);
        drop(edit);

        let erase = RangeEdit {
            start: 4096,
            end: 4096 + 65536,
            replacement: f.own(b""),
        };
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        permit
            .edit_file_range_stream(handle, after.stamp, &erase, &[], deadline())
            .unwrap();
        drop(permit);
        assert_eq!(f.read(handle, 0, 8192), original);
        drop(erase);

        let mixed = RangeEdit {
            start: 100,
            end: 104,
            replacement: f.own(b"ABCDWXYZ"),
        };
        let before = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        permit
            .edit_file_range_stream(
                handle,
                before.stamp,
                &mixed,
                &[RangePart::Bytes(4), RangePart::Zero(5), RangePart::Bytes(4)],
                deadline(),
            )
            .unwrap();
        drop(permit);
        let after = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        assert_eq!(after.stamp.revision, before.stamp.revision + 1);
        assert_eq!(after.length, 8201);
        assert_eq!(f.read(handle, 100, 13), b"ABCD\0\0\0\0\0WXYZ");
        drop(mixed);

        let restore = RangeEdit {
            start: 100,
            end: 113,
            replacement: f.own(&original[100..104]),
        };
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        permit
            .edit_file_range_stream(
                handle,
                after.stamp,
                &restore,
                &[RangePart::Bytes(4)],
                deadline(),
            )
            .unwrap();
        drop(permit);
        assert_eq!(f.read(handle, 0, 8192), original);
        drop(restore);

        f.workspace.release(handle).unwrap();
        mount.finish().unwrap();
        f.workspace.commit(deadline()).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("PROJECTED_RANGE_CHECK stream PASS");
    }

    #[test]
    #[ignore = "requires native service and pristine Chunked base"]
    fn projected_range_zero_capacity() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        let mut mount = f.workspace.reserve_mount().unwrap();
        mount.bind_invalidation(Arc::new(|_, _, _| Ok(()))).unwrap();
        let handle = open(&f, file.serial, FileAccess::ReadWrite, false);
        let before = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        let zero = RangeEdit {
            start: before.length,
            end: before.length,
            replacement: f.own(b""),
        };
        let backing = f.workspace.backing_status().unwrap();
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit.edit_file_range_stream(
                handle,
                before.stamp,
                &zero,
                &[RangePart::Zero(8 * 1024 * 1024 + 1)],
                deadline()
            ),
            Err(WorkspaceError::Capacity)
        );
        drop(permit);
        assert_eq!(
            f.workspace
                .projected_range_state(handle, file.serial)
                .unwrap(),
            before
        );
        assert_eq!(
            f.workspace.backing_status().unwrap().allocated_bytes,
            backing.allocated_bytes
        );
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit
                .edit_file_range_stream(
                    handle,
                    before.stamp,
                    &zero,
                    &[RangePart::Zero(8 * 1024 * 1024)],
                    deadline()
                )
                .unwrap()
                .accepted_bytes,
            0
        );
        drop(permit);
        let after = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        assert_eq!(after.stamp.revision, before.stamp.revision + 1);
        assert_eq!(after.length, before.length + 8 * 1024 * 1024);
        assert_eq!(f.read(handle, before.length, 4096), vec![0; 4096]);
        assert_eq!(f.read(handle, after.length - 16, 16), vec![0; 16]);
        assert_eq!(f.read(handle, after.length, 1), Vec::<u8>::new());
        f.workspace.release(handle).unwrap();
        mount.finish().unwrap();
        f.workspace.commit(deadline()).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        drop(zero);
        f.workspace.close_clean().unwrap();
        println!("PROJECTED_RANGE_CHECK zero_capacity PASS");
    }

    #[test]
    #[ignore = "requires root-owned native fixture and a non-root configured Workspace owner"]
    fn projected_range_nonroot_mode() {
        let f = nonroot_fixture();
        let file = f.lookup(b"data.bin");
        let mode = |value| PortableAttributes {
            mode: Some(value),
            ..PortableAttributes::default()
        };
        f.workspace
            .set_attributes(file.serial, mode(0o400), deadline())
            .unwrap();
        f.workspace.commit(deadline()).unwrap();
        assert_eq!(
            f.workspace.getattr(file.serial).unwrap().mode & 0o777,
            0o400
        );
        f.workspace
            .set_attributes(file.serial, mode(0o600), deadline())
            .unwrap();
        assert_eq!(
            f.workspace.getattr(file.serial).unwrap().mode & 0o777,
            0o600
        );

        let mut mount = f.workspace.reserve_mount().unwrap();
        mount.bind_invalidation(Arc::new(|_, _, _| Ok(()))).unwrap();
        let handle = open(&f, file.serial, FileAccess::ReadWrite, false);
        let stamp = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap()
            .stamp;
        let edit = RangeEdit {
            start: 0,
            end: 0,
            replacement: f.own(b"Z"),
        };
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit
                .edit_file_range(handle, stamp, &edit, deadline())
                .unwrap()
                .accepted_bytes,
            1
        );
        drop(permit);
        assert_eq!(f.read(handle, 0, 1), b"Z");
        f.workspace.release(handle).unwrap();
        mount.finish().unwrap();
        f.workspace.commit(deadline()).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        drop(edit);
        f.workspace.close_clean().unwrap();
        println!("PROJECTED_RANGE_CHECK nonroot_mode PASS");
    }

    #[test]
    #[ignore = "requires native service and private Workspace backing"]
    fn projected_range_notification_failure() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        f.workspace.set_len(file.serial, 8192, deadline()).unwrap();
        let mut mount = f.workspace.reserve_mount().unwrap();
        mount
            .bind_invalidation(Arc::new(|_, _, _| {
                Err(io::Error::other("notification failed"))
            }))
            .unwrap();
        let handle = open(&f, file.serial, FileAccess::ReadWrite, false);
        let before = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        let edit = RangeEdit {
            start: 4093,
            end: 4093,
            replacement: f.own(b"ABCD"),
        };
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        let WorkspaceError::Coherence(failure) = permit
            .edit_file_range(handle, before.stamp, &edit, deadline())
            .unwrap_err()
        else {
            panic!("published mutation must retain its receipt");
        };
        drop(permit);
        assert_eq!(failure.receipt.accepted_bytes, 4);
        assert_eq!(failure.receipt.revision, before.stamp.revision + 1);
        let after = f
            .workspace
            .projected_range_state(handle, file.serial)
            .unwrap();
        assert_eq!(after.length, 8196);
        assert!(!after.writable);
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Failed(failure))
        );
        assert_eq!(
            f.workspace.status().unwrap().range_accepted_payload_bytes,
            4
        );
        assert_eq!(f.workspace.status().unwrap().range_shifted_suffix_bytes, 0);
        f.workspace.release(handle).unwrap();
        mount.finish().unwrap();
        f.workspace.commit(deadline()).unwrap();
        f.workspace
            .forget(file.serial, u64::MAX, ReferenceScope::Local);
        drop(edit);
        f.workspace.close_clean().unwrap();
        println!("PROJECTED_RANGE_CHECK notification_failure PASS");
    }
}
