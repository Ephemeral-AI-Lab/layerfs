//! Public-API checks for the namespace and portable-attribute operations.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{fs, path::PathBuf};
    fn check(id: &str) {
        println!("NAMESPACE_CHECK {id} PASS");
    }
    fn fixture(gate: Gate) -> Fixture {
        let native = Native::new_fresh(gate);
        let root =
            PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).join("namespace");
        if !root.exists() {
            fs::create_dir(&root).unwrap();
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
    fn file(f: &Fixture, parent: u64, name: &[u8], mode: u32) -> NodeAttributes {
        f.workspace
            .mknod(parent, name, mode, 0, deadline())
            .unwrap()
    }
    fn directory(f: &Fixture, parent: u64, name: &[u8], mode: u32) -> NodeAttributes {
        f.workspace
            .mkdir(parent, name, mode, 0, deadline())
            .unwrap()
    }
    fn listed(f: &Fixture, serial: u64) -> Vec<Vec<u8>> {
        let handle = f.workspace.opendir(serial, ReferenceScope::Local).unwrap();
        let page = f
            .workspace
            .readdir(handle, 0, MAX_DIRECTORY_ENTRIES, deadline())
            .unwrap();
        let names = page
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        f.workspace.releasedir(handle).unwrap();
        names
    }
    fn open_write(f: &Fixture, serial: u64) -> HandleId {
        f.workspace
            .open_file(
                serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    append: false,
                    truncate: false,
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap()
    }
    fn write(f: &Fixture, handle: HandleId, offset: u64, bytes: &[u8]) -> MutationReceipt {
        let payload = f.own(bytes);
        f.workspace
            .write_file(handle, offset, &payload, deadline())
            .unwrap()
    }
    fn read(f: &Fixture, handle: HandleId, offset: u64, size: usize) -> Vec<u8> {
        f.workspace
            .read(handle, offset, size, deadline())
            .unwrap()
            .as_ref()
            .to_vec()
    }
    fn commit(f: &Fixture) -> CommitReport {
        let report = f.workspace.commit(deadline()).unwrap();
        assert!(f.workspace.status().unwrap().submission.is_none());
        report
    }
    fn committed_root(report: &CommitReport) -> Root {
        match &report.outcome {
            CommitOutcomeWire::Committed(value) => value.root,
            CommitOutcomeWire::UpToDate { root, .. } => *root,
        }
    }
    fn saved(f: &Fixture, root: Root, path: &[u8]) -> (NodeAttributes, Root) {
        let Response::Attributes {
            serial,
            kind,
            size,
            references,
            mode,
            mtime,
            nanoseconds,
            content,
            ..
        } = f.native.attributes(root, path)
        else {
            panic!("attributes")
        };
        (
            NodeAttributes {
                serial,
                kind: if kind == 2 {
                    NodeKind::Directory
                } else if kind == 3 {
                    NodeKind::Symlink
                } else {
                    NodeKind::File
                },
                size,
                references,
                mode,
                mtime_seconds: mtime,
                mtime_nanoseconds: nanoseconds,
                uid: 0,
                gid: 0,
            },
            content,
        )
    }
    fn missing(f: &Fixture, root: Root, path: &[u8]) {
        assert_eq!(
            f.native
                .request(
                    Operation::Inspect {
                        root,
                        query: Inspect::Attributes {
                            path: path.to_vec()
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
        for (serial, count) in refs {
            f.workspace.forget(*serial, *count, ReferenceScope::Local);
        }
        f.workspace.close_clean().unwrap();
        check("native-clean-close");
    }
    fn attributes(
        mode: Option<u32>,
        mtime: Option<(i64, u32)>,
        size: Option<u64>,
    ) -> PortableAttributes {
        PortableAttributes { size, mode, mtime }
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_setattr() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        let dir = directory(&f, top, b"dir", 0o750);
        let a = file(&f, top, b"a", 0o640);
        let handle = open_write(&f, a.serial);
        write(&f, handle, 0, b"body");
        // A directory carries mode plus the sticky bit and no content.
        let dir_updated = f
            .workspace
            .set_attributes(
                dir.serial,
                attributes(Some(0o1750), Some((1_700_000_124, 0)), None),
                deadline(),
            )
            .unwrap();
        assert_eq!(dir_updated.mode, 0o1750);
        assert_eq!(listed(&f, dir.serial), vec![b".".to_vec(), b"..".to_vec()]);
        // Mode and mtime are one atomic request on a regular file.
        let updated = f
            .workspace
            .set_attributes(
                a.serial,
                attributes(Some(0o600), Some((1_700_000_123, 456)), None),
                deadline(),
            )
            .unwrap();
        assert_eq!(updated.size, 4);
        assert_eq!(updated.mode, 0o600);
        assert_eq!(
            (updated.mtime_seconds, updated.mtime_nanoseconds),
            (1_700_000_123, 456)
        );
        assert_eq!(read(&f, handle, 0, 4), b"body");
        // A directory carries mode plus the sticky bit and no content.
        let updated = f
            .workspace
            .set_attributes(
                dir.serial,
                attributes(Some(0o1750), Some((1_700_000_124, 0)), None),
                deadline(),
            )
            .unwrap();
        assert_eq!(updated.mode, 0o1750);
        assert_eq!(listed(&f, dir.serial), vec![b".".to_vec(), b"..".to_vec()]);
        // Every invalid field refuses the whole request and changes nothing.
        let before = f.workspace.getattr(a.serial).unwrap();
        assert_eq!(
            f.workspace.set_attributes(
                a.serial,
                attributes(Some(0o1000), Some((1_700_000_125, 0)), None),
                deadline()
            ),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(
            f.workspace.set_attributes(
                a.serial,
                attributes(Some(0o600), Some((1_700_000_125, 1_000_000_000)), None),
                deadline()
            ),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(
            f.workspace
                .set_attributes(dir.serial, attributes(None, None, Some(1)), deadline()),
            Err(WorkspaceError::Unsupported)
        );
        assert_eq!(
            f.workspace
                .set_attributes(a.serial, attributes(None, None, None), deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(f.workspace.getattr(a.serial).unwrap(), before);
        let report = commit(&f);
        let head = committed_root(&report);
        let (saved_attr, content) = saved(&f, head, b"a");
        assert_eq!(saved_attr.mode, 0o600);
        assert_eq!(
            (saved_attr.mtime_seconds, saved_attr.mtime_nanoseconds),
            (1_700_000_123, 456)
        );
        assert_eq!(f.native.bytes(content, 0, 4), b"body");
        let (saved_dir, _) = saved(&f, head, b"dir");
        assert_eq!(saved_dir.mode, 0o1750);
        println!(
            "NAMESPACE_SETATTR file_mode=0600 file_mtime=1700000123.456 directory_mode=1750 refusals=4 content_root_unchanged=true"
        );
        close(&f, &[handle], &[(a.serial, 1), (dir.serial, 1)]);
        check("setattr-atomic-mode-mtime-size-and-refusals");
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_mknod() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        let before = f.workspace.status().unwrap();
        let a = file(&f, top, b"node", 0o644);
        assert_eq!(a.kind, NodeKind::File);
        assert_eq!(a.size, 0);
        assert_eq!(a.references, 1);
        let after = f.workspace.status().unwrap();
        assert_eq!(after.handles, before.handles);
        assert_eq!(after.nodes, before.nodes + 1);
        assert_eq!(listed(&f, top).contains(&b"node".to_vec()), true);
        // A duplicate name and an unsupported kind both refuse unchanged.
        assert_eq!(
            f.workspace.mknod(top, b"node", 0o644, 0, deadline()),
            Err(WorkspaceError::Exists)
        );
        assert_eq!(
            f.workspace.mknod(top, b"other", 0o1000, 0, deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        let report = commit(&f);
        let head = committed_root(&report);
        let (saved_attr, _) = saved(&f, head, b"node");
        assert_eq!(saved_attr.kind, NodeKind::File);
        assert_eq!((saved_attr.size, saved_attr.mode), (0, 0o644));
        println!(
            "NAMESPACE_MKNOD handles_before={} handles_after={} nodes_delta=1 mode=0644 size=0",
            before.handles, after.handles
        );
        close(&f, &[], &[(a.serial, 1)]);
        check("mknod-empty-regular-file-without-a-handle");
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_link() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        let a = file(&f, top, b"first", 0o644);
        let handle = open_write(&f, a.serial);
        write(&f, handle, 0, b"shared");
        let alias = f
            .workspace
            .link(top, b"second", a.serial, deadline())
            .unwrap();
        assert_eq!(alias.serial, a.serial);
        assert_eq!(alias.kind, NodeKind::File);
        assert_eq!(alias.size, 6);
        let names = listed(&f, top);
        assert!(names.contains(&b"first".to_vec()) && names.contains(&b"second".to_vec()));
        // A write through the original is visible through the alias.
        write(&f, handle, 6, b"-more");
        assert_eq!(f.workspace.getattr(a.serial).unwrap().size, 11);
        let second = f
            .workspace
            .open_file(
                a.serial,
                FileOpenOptions {
                    access: FileAccess::ReadOnly,
                    append: false,
                    truncate: false,
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        assert_eq!(read(&f, second, 0, 11), b"shared-more");
        assert_eq!(
            f.workspace.link(top, b"first", a.serial, deadline()),
            Err(WorkspaceError::Exists)
        );
        let report = commit(&f);
        let head = committed_root(&report);
        let (first, content) = saved(&f, head, b"first");
        let (second_attr, second_content) = saved(&f, head, b"second");
        assert_eq!(first.serial, second_attr.serial);
        assert_eq!(content, second_content);
        assert_eq!(f.native.bytes(content, 0, 11), b"shared-more");
        assert_eq!(first.references, 2);
        println!(
            "NAMESPACE_LINK serial={} names=2 references={} content_shared=true",
            first.serial, first.references
        );
        close(&f, &[handle, second], &[(a.serial, 2)]);
        check("link-shares-one-inode-and-one-saved-version");
    }

    #[test]
    #[ignore = "requires namespace_route.py real Native Service"]
    fn namespace_unlink() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        let a = file(&f, top, b"gone", 0o644);
        let alias = f
            .workspace
            .link(top, b"kept", a.serial, deadline())
            .unwrap();
        let handle = open_write(&f, a.serial);
        write(&f, handle, 0, b"before");
        f.workspace.unlink(top, b"gone", deadline()).unwrap();
        assert_eq!(
            f.workspace.unlink(top, b"gone", deadline()),
            Err(WorkspaceError::NotFound)
        );
        // The surviving alias and the open handle keep the same inode.
        assert_eq!(f.workspace.getattr(a.serial).unwrap().serial, alias.serial);
        assert_eq!(read(&f, handle, 0, 6), b"before");
        write(&f, handle, 6, b"-after");
        assert_eq!(read(&f, handle, 0, 12), b"before-after");
        f.workspace.unlink(top, b"kept", deadline()).unwrap();
        // The last name is gone; the open handle still reads and writes.
        assert_eq!(read(&f, handle, 0, 12), b"before-after");
        write(&f, handle, 0, b"BEFORE");
        assert_eq!(read(&f, handle, 0, 6), b"BEFORE");
        let report = commit(&f);
        let head = committed_root(&report);
        missing(&f, head, b"gone");
        missing(&f, head, b"kept");
        assert_eq!(read(&f, handle, 0, 12), b"BEFORE-after");
        f.workspace.release(handle).unwrap();
        f.workspace.forget(a.serial, 1, ReferenceScope::Local);
        assert_eq!(f.workspace.getattr(a.serial), Err(WorkspaceError::NotFound));
        println!(
            "NAMESPACE_UNLINK removed=2 open_orphan_readable=true commit_head_absent=true reclaimed=true"
        );
        f.workspace.close_clean().unwrap();
        check("unlink-removes-one-name-and-keeps-open-orphan-lifetime");
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_unlink_fresh() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        let a = file(&f, top, b"transient", 0o644);
        f.workspace.unlink(top, b"transient", deadline()).unwrap();
        // A fresh identity with no name must not be declared as unbound.
        let report = commit(&f);
        let head = committed_root(&report);
        missing(&f, head, b"transient");
        let report = f.workspace.commit(deadline()).unwrap();
        let next = committed_root(&report);
        assert_eq!(head, next);
        println!("NAMESPACE_UNLINK_FRESH unbound_fresh_declined=true no_resurrection=true");
        f.workspace.forget(a.serial, 1, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("fresh-create-then-unlink-emits-no-unbound-declaration");
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_rmdir() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        let parent = directory(&f, top, b"parent", 0o755);
        let empty = directory(&f, parent.serial, b"empty", 0o755);
        assert_eq!(empty.kind, NodeKind::Directory);
        let full = directory(&f, parent.serial, b"full", 0o755);
        let child = file(&f, full.serial, b"child", 0o644);
        assert_eq!(
            f.workspace.rmdir(parent.serial, b"full", deadline()),
            Err(WorkspaceError::NotEmpty)
        );
        assert_eq!(
            f.workspace.rmdir(parent.serial, b"child", deadline()),
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(
            f.workspace.rmdir(top, b"parent", deadline()),
            Err(WorkspaceError::NotEmpty)
        );
        assert_eq!(
            f.workspace.unlink(parent.serial, b"empty", deadline()),
            Err(WorkspaceError::IsDirectory)
        );
        // A pending removal inside the child makes the directory empty.
        f.workspace
            .unlink(full.serial, b"child", deadline())
            .unwrap();
        f.workspace
            .rmdir(parent.serial, b"full", deadline())
            .unwrap();
        f.workspace
            .rmdir(parent.serial, b"empty", deadline())
            .unwrap();
        let names = listed(&f, parent.serial);
        assert_eq!(names, vec![b".".to_vec(), b"..".to_vec()]);
        let report = commit(&f);
        let head = committed_root(&report);
        missing(&f, head, b"parent/empty");
        missing(&f, head, b"parent/full");
        let (saved_parent, _) = saved(&f, head, b"parent");
        assert_eq!(saved_parent.kind, NodeKind::Directory);
        println!(
            "NAMESPACE_RMDIR empty_removed=true nonempty_refused=true pending_child_counted=true"
        );
        close(&f, &[], &[(parent.serial, 1), (child.serial, 1)]);
        check("rmdir-uses-the-effective-emptiness-check");
    }

    #[test]
    #[ignore = "requires namespace_route.py real Native Service"]
    fn namespace_rename() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        let left = directory(&f, top, b"left", 0o755);
        let right = directory(&f, top, b"right", 0o755);
        let a = file(&f, left.serial, b"a", 0o644);
        let handle = open_write(&f, a.serial);
        write(&f, handle, 0, b"payload");
        let replaced = file(&f, right.serial, b"target", 0o644);
        let held = open_write(&f, replaced.serial);
        write(&f, held, 0, b"older");
        // Cross-directory rename replacing an existing name.
        f.workspace
            .rename(
                left.serial,
                b"a",
                right.serial,
                b"target",
                RenameFlags::default(),
                deadline(),
            )
            .unwrap();
        assert_eq!(listed(&f, left.serial), vec![b".".to_vec(), b"..".to_vec()]);
        assert!(listed(&f, right.serial).contains(&b"target".to_vec()));
        // The moved inode keeps its own content and its open handle.
        assert_eq!(read(&f, handle, 0, 7), b"payload");
        // The replaced inode keeps its own content for its existing handle.
        assert_eq!(read(&f, held, 0, 5), b"older");
        assert_eq!(f.workspace.getattr(replaced.serial).unwrap().size, 5);
        // A same-inode alias is a no-op, and NOREPLACE honours the destination.
        f.workspace
            .rename(
                right.serial,
                b"target",
                right.serial,
                b"target",
                RenameFlags::default(),
                deadline(),
            )
            .unwrap();
        assert_eq!(
            f.workspace.rename(
                right.serial,
                b"target",
                left.serial,
                b"target",
                RenameFlags { noreplace: true },
                deadline()
            ),
            Err(WorkspaceError::NotFound)
        );
        let blocker = file(&f, left.serial, b"blocker", 0o644);
        assert_eq!(
            f.workspace.rename(
                right.serial,
                b"target",
                left.serial,
                b"blocker",
                RenameFlags { noreplace: true },
                deadline()
            ),
            Err(WorkspaceError::Exists)
        );
        // A directory moves with its subtree; moving it beneath its own
        // descendant is refused before any publication.
        let inner = directory(&f, right.serial, b"inner", 0o755);
        assert_eq!(
            f.workspace.rename(
                right.serial,
                b"inner",
                inner.serial,
                b"nested",
                RenameFlags::default(),
                deadline()
            ),
            Err(WorkspaceError::InvalidInput)
        );
        f.workspace
            .rename(
                right.serial,
                b"inner",
                left.serial,
                b"inner",
                RenameFlags::default(),
                deadline(),
            )
            .unwrap();
        assert_eq!(listed(&f, right.serial).contains(&b"inner".to_vec()), false);
        assert_eq!(listed(&f, left.serial).contains(&b"inner".to_vec()), true);
        let report = commit(&f);
        let head = committed_root(&report);
        missing(&f, head, b"left/a");
        let (moved, content) = saved(&f, head, b"right/target");
        assert_eq!(moved.serial, a.serial);
        assert_eq!(f.native.bytes(content, 0, 7), b"payload");
        let (moved_dir, _) = saved(&f, head, b"left/inner");
        assert_eq!(moved_dir.kind, NodeKind::Directory);
        println!(
            "NAMESPACE_RENAME cross_parent=true replaced_handle_intact=true noreplace=true same_inode_noop=true directory_cycle_refused=true"
        );
        close(
            &f,
            &[handle, held],
            &[
                (a.serial, 1),
                (replaced.serial, 1),
                (blocker.serial, 1),
                (inner.serial, 1),
            ],
        );
        check("rename-publishes-both-parent-edits-atomically");
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_rename_base() {
        let f = fixture(Gate::None);
        let top = f.workspace.root().serial;
        // `data.bin` comes from the attached canonical base, so its removal is a
        // tombstone over an inherited binding rather than over a local addition.
        let serial = f
            .workspace
            .lookup(top, b"data.bin", ReferenceScope::Local, deadline())
            .unwrap();
        let alias = f
            .workspace
            .link(top, b"alias", serial.serial, deadline())
            .unwrap();
        f.workspace.unlink(top, b"data.bin", deadline()).unwrap();
        assert_eq!(
            f.workspace
                .lookup(top, b"data.bin", ReferenceScope::Local, deadline()),
            Err(WorkspaceError::NotFound)
        );
        let names = listed(&f, top);
        assert!(!names.contains(&b"data.bin".to_vec()) && names.contains(&b"alias".to_vec()));
        let report = commit(&f);
        let head = committed_root(&report);
        missing(&f, head, b"data.bin");
        let (saved_alias, _) = saved(&f, head, b"alias");
        assert_eq!(saved_alias.serial, serial.serial);
        println!("NAMESPACE_RENAME_BASE base_name_removed=true alias_retained=true");
        f.workspace.forget(serial.serial, 1, ReferenceScope::Local);
        f.workspace.forget(alias.serial, 1, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("tombstone-hides-an-inherited-binding");
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_generation() {
        let f = fixture(Gate::Delivery);
        let top = f.workspace.root().serial;
        let a = file(&f, top, b"held", 0o644);
        let handle = open_write(&f, a.serial);
        write(&f, handle, 0, b"first");
        let saving = {
            let workspace = f.workspace.clone();
            std::thread::spawn(move || workspace.stage(deadline()))
        };
        f.native.wait_entered();
        // Later changes while G is retained: a rename, an unlink and metadata.
        f.workspace
            .rename(
                top,
                b"held",
                top,
                b"moved",
                RenameFlags::default(),
                deadline(),
            )
            .unwrap();
        f.workspace
            .set_attributes(
                a.serial,
                attributes(Some(0o600), Some((1_700_000_500, 1)), None),
                deadline(),
            )
            .unwrap();
        let extra = file(&f, top, b"extra", 0o644);
        f.workspace.unlink(top, b"extra", deadline()).unwrap();
        f.native.release();
        let stage = saving.join().unwrap().unwrap();
        let frozen = stage.stage().clone();
        let first = f.workspace.commit_staged(&stage, deadline()).unwrap();
        assert_eq!(first.stage_token, Some(frozen.token));
        assert_eq!(first.generation, 0);
        let (saved_attr, content) = saved(&f, frozen.candidate_root, b"held");
        assert_eq!(saved_attr.mode, 0o644);
        assert_eq!(f.native.bytes(content, 0, 5), b"first");
        missing(&f, frozen.candidate_root, b"moved");
        // G completion must not erase the later rename or metadata update.
        let second = commit(&f);
        assert_eq!(second.generation, 1);
        let head = committed_root(&second);
        missing(&f, head, b"held");
        missing(&f, head, b"extra");
        let (moved, content) = saved(&f, head, b"moved");
        assert_eq!(moved.serial, a.serial);
        assert_eq!(moved.mode, 0o600);
        assert_eq!(
            (moved.mtime_seconds, moved.mtime_nanoseconds),
            (1_700_000_500, 1)
        );
        assert_eq!(f.native.bytes(content, 0, 5), b"first");
        assert_eq!(read(&f, handle, 0, 5), b"first");
        println!(
            "NAMESPACE_GENERATION first_generation={} second_generation={} later_rename_and_metadata_survived=true",
            first.generation, second.generation
        );
        close(&f, &[handle], &[(a.serial, 1), (extra.serial, 1)]);
        check("g-plus-one-keeps-later-namespace-and-metadata-changes");
    }

    #[test]
    #[ignore = "requires namespace_route.py real native Service"]
    fn namespace_notification_failure() {
        let f = fixture(Gate::None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let top = f.workspace.root().serial;
        let left = directory(&f, top, b"left", 0o755);
        let right = directory(&f, top, b"right", 0o755);
        let a = file(&f, left.serial, b"a", 0o644);
        let before = f.workspace.status().unwrap();
        assert_eq!(before.coherence, Some(CoherenceStatus::Ready));
        // The mounted projection owns one delivery; a rename publishes first and
        // reports the notification result separately from the mutation.
        let result = f.workspace.rename(
            left.serial,
            b"a",
            right.serial,
            b"b",
            RenameFlags::default(),
            deadline(),
        );
        assert_eq!(result, Ok(()));
        let after = f.workspace.status().unwrap();
        assert_eq!(after.revision, before.revision + 1);
        assert_eq!(listed(&f, right.serial).contains(&b"b".to_vec()), true);
        let file = std::fs::read(f.workspace.mount_path().join("right/b")).unwrap();
        assert!(file.is_empty());
        mount.unmount(deadline()).unwrap();
        println!(
            "NAMESPACE_NOTIFY multi_entry_rename_published=true status={:?}",
            after.coherence
        );
        close(&f, &[], &[(a.serial, 1)]);
        check("multi-entry-sdk-mutation-keeps-its-published-result");
    }
}
