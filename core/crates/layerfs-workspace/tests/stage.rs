//! Public staging through the actual native service and Linux private backing.
#[path = "support/prepared.rs"]
mod prepared_stream;
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::{prepared_stream, support::*};
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        io::Write,
        os::unix::fs::{FileExt, MetadataExt},
        time::{Duration, Instant},
    };
    fn physical_private_files(root: &std::path::Path) -> (u64, usize) {
        let mut pending = vec![root.to_path_buf()];
        let mut blocks = 0;
        let mut packs = 0;
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                let metadata = entry.metadata().unwrap();
                if metadata.is_dir() {
                    pending.push(entry.path());
                } else if metadata.is_file() {
                    blocks += metadata.blocks() * 512;
                    packs +=
                        usize::from(entry.file_name().to_string_lossy().starts_with("a-pack-v1"));
                }
            }
        }
        (blocks, packs)
    }
    fn private_files_with_prefix(root: &std::path::Path, prefix: &str) -> Vec<String> {
        let mut pending = vec![root.to_path_buf()];
        let mut packs = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                } else if entry.file_name().to_string_lossy().starts_with(prefix) {
                    packs.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        packs.sort();
        packs
    }
    fn private_pack_files(root: &std::path::Path) -> Vec<String> {
        private_files_with_prefix(root, "a-pack-v1")
    }
    fn private_file_path(root: &std::path::Path, name: &str) -> std::path::PathBuf {
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                } else if entry.file_name() == name {
                    return entry.path();
                }
            }
        }
        panic!("private page missing: {name}")
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_generation() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        let held = f
            .workspace
            .open(file.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"G1G1");
        let first = f.workspace.stage(deadline()).unwrap();
        f.edit(b"data.bin", 10, 14, b"G2G2");
        let staged = attr(
            f.native
                .attributes(first.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(
            f.native.bytes(staged.1, 8, 8),
            [8, 9, b'G', b'1', b'G', b'1', 14, 15]
        );
        assert_eq!(f.read(held, 10, 4), b"G2G2");
        check("active-g1-staged-bytes-and-g2-live-bytes");
        f.workspace.commit_staged(&first, deadline()).unwrap();
        assert_eq!(f.read(held, 10, 4), b"G2G2");
        let second = f.workspace.commit(deadline()).unwrap();
        let branch = f.branch();
        let Response::History(result) = branch else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        assert_eq!(
            f.native.bytes(saved.1, 8, 8),
            [8, 9, b'G', b'2', b'G', b'2', 14, 15]
        );
        let outcome_root = match second.outcome {
            CommitOutcomeWire::Committed(commit) => commit.root,
            CommitOutcomeWire::UpToDate { root, .. } => root,
        };
        assert_eq!(outcome_root, branch.effective_root);
        f.workspace.release(held).unwrap();
        check("active-g1-g2-commits-and-final-bytes");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_close() {
        let f = Fixture::new_fresh(Gate::None);
        let (file, handle) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"created",
                FileCreateOptions {
                    mode: 0o644,
                    umask: 0,
                    exclusive: true,
                    open: FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                },
                deadline(),
            )
            .unwrap();
        f.workspace
            .write_file(handle, 0, &f.own(b"x"), deadline())
            .unwrap();
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        f.workspace.commit(deadline()).unwrap();
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"created"));
        assert_eq!(f.native.bytes(saved.1, 0, 1), b"x");
        let root =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let (physical, packs) = physical_private_files(&root);
        let charged = f.workspace.backing_status().unwrap().allocated_bytes;
        let metadata = f.workspace.metadata_status().unwrap().allocated_bytes;
        assert!(metadata <= charged);
        assert_eq!(physical, charged);
        assert_eq!(packs, 0);
        println!(
            "STAGE_ALLOCATION physical_bytes={physical} charged_bytes={charged} metadata_bytes={metadata} pack_pages={packs}"
        );
        check("active-fresh-name-and-file-commit");
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&root), (0, 0));
        check("active-committed-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_namespace() {
        let f = Fixture::new_fresh(Gate::None);
        let root = f.workspace.root().serial;
        let directory = f
            .workspace
            .mkdir(root, b"local", 0o755, 0, deadline())
            .unwrap();
        let (file, handle) = f
            .workspace
            .create_file(
                directory.serial,
                b"file",
                FileCreateOptions {
                    mode: 0o644,
                    umask: 0,
                    exclusive: true,
                    open: FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                },
                deadline(),
            )
            .unwrap();
        f.workspace
            .write_file(handle, 0, &f.own(b"abc"), deadline())
            .unwrap();
        f.workspace.release(handle).unwrap();
        f.workspace
            .link(root, b"new-alias", file.serial, deadline())
            .unwrap();
        f.workspace
            .symlink(root, b"shortcut", b"local/file", deadline())
            .unwrap();
        f.workspace.commit(deadline()).unwrap();
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let by_path = attr(f.native.attributes(branch.effective_root, b"local/file"));
        let by_alias = attr(f.native.attributes(branch.effective_root, b"new-alias"));
        assert_eq!(by_path, by_alias);
        assert_eq!(f.native.bytes(by_path.1, 0, 3), b"abc");
        assert_eq!(by_path.0, file.serial);
        check("active-fresh-directory-link-and-symlink-commit");
        f.workspace
            .rename(
                root,
                b"new-alias",
                root,
                b"renamed",
                RenameFlags::default(),
                deadline(),
            )
            .unwrap();
        f.workspace.unlink(root, b"shortcut", deadline()).unwrap();
        f.workspace.commit(deadline()).unwrap();
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let moved = attr(f.native.attributes(branch.effective_root, b"renamed"));
        assert_eq!(moved.0, file.serial);
        assert_eq!(f.native.bytes(moved.1, 0, 3), b"abc");
        check("active-successor-rename-and-unlink-commit");
    }
    #[test]
    #[ignore = "requires privileged stage_route.py with /dev/fuse"]
    fn stage_active_mounted() {
        let f = Fixture::new_fresh(Gate::None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().join("mounted");
        std::fs::write(&path, b"mount").unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"ed")
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"mounted");
        mount.unmount(deadline()).unwrap();
        check("active-mounted-fuse-write-unmount");
        f.workspace.commit(deadline()).unwrap();
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"mounted"));
        assert_eq!(f.native.bytes(saved.1, 0, 7), b"mounted");
        check("active-mounted-public-commit-bytes");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_many_file() {
        let f = Fixture::new_fresh(Gate::None);
        let root = f.workspace.root().serial;
        for index in 0..128u8 {
            let name = format!("m{index:03}");
            let (file, handle) = f
                .workspace
                .create_file(
                    root,
                    name.as_bytes(),
                    FileCreateOptions {
                        mode: 0o644,
                        umask: 0,
                        exclusive: true,
                        open: FileOpenOptions {
                            access: FileAccess::ReadWrite,
                            ..FileOpenOptions::default()
                        },
                    },
                    deadline(),
                )
                .unwrap();
            f.workspace
                .write_file(handle, 0, &f.own(&[index]), deadline())
                .unwrap();
            f.workspace.release(handle).unwrap();
            f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        }
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let (before, packs_before) = physical_private_files(&private);
        assert_eq!(
            before,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        assert!(packs_before <= 3);
        check("active-128-files-shared-pack-before-commit");
        f.workspace.commit(deadline()).unwrap();
        let (after, packs_after) = physical_private_files(&private);
        assert_eq!(after, f.workspace.backing_status().unwrap().allocated_bytes);
        assert_eq!(packs_after, 0);
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        for index in 0..128u8 {
            let name = format!("m{index:03}");
            let saved = attr(f.native.attributes(branch.effective_root, name.as_bytes()));
            assert_eq!(f.native.bytes(saved.1, 0, 1), [index]);
        }
        println!("STAGE_ALLOCATION case=128 before_bytes={before} after_bytes={after} before_pack_pages={packs_before} after_pack_pages={packs_after}");
        check("active-128-files-commit-and-byte-oracle");
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-128-files-exact-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_repeated() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        for index in 0..4097usize {
            f.workspace
                .write_file(handle, 8, &f.own(&[b'B' + (index % 24) as u8]), deadline())
                .unwrap();
        }
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let (before, packs_before) = physical_private_files(&private);
        assert_eq!(
            before,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        check("active-4097-repeated-public-backing");
        let counts_before = f.workspace.backing_status().unwrap();
        let started = Instant::now();
        f.workspace.commit(deadline()).unwrap();
        let commit_wall = started.elapsed();
        let counts_after = f.workspace.backing_status().unwrap();
        assert!(counts_after.active_pack_fetches > counts_before.active_pack_fetches);
        println!("STAGE_PHASE case=repeated4097 commit_wall_ns={} pack_fetches={} index_fetches={} pack_page_writes={} index_page_writes={} cache_claim=none", commit_wall.as_nanos(), counts_after.active_pack_fetches - counts_before.active_pack_fetches, counts_after.active_index_fetches - counts_before.active_index_fetches, counts_after.active_pack_page_writes - counts_before.active_pack_page_writes, counts_after.active_index_page_writes - counts_before.active_index_page_writes);
        let (after, packs_after) = physical_private_files(&private);
        assert_eq!(after, f.workspace.backing_status().unwrap().allocated_bytes);
        assert_eq!(packs_after, 0);
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 8, 1), [b'B' + (4096 % 24) as u8]);
        println!("STAGE_ALLOCATION case=repeated4097 before_bytes={before} after_bytes={after} before_pack_pages={packs_before} after_pack_pages={packs_after}");
        check("active-4097-repeated-commit-and-byte-oracle");
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-4097-repeated-exact-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_retained32() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let mut pins = Vec::new();
        let mut first_root = None;
        for index in 0..32u8 {
            f.workspace
                .write_file(handle, 0, &f.own(&[b'B' + index]), deadline())
                .unwrap();
            pins.push(
                f.workspace
                    .opendir(f.workspace.root().serial, ReferenceScope::Local)
                    .unwrap(),
            );
            f.workspace.commit(deadline()).unwrap();
            if index == 0 {
                let Response::History(result) = f.branch() else {
                    panic!("branch result")
                };
                let HistoryResult::BranchSnapshot(branch) = *result else {
                    panic!("branch snapshot")
                };
                first_root = Some(branch.effective_root);
            }
        }
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let (retained, retained_packs) = physical_private_files(&private);
        assert_eq!(
            retained,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        assert!(retained_packs >= 31);
        let first = attr(f.native.attributes(first_root.unwrap(), b"data.bin"));
        assert_eq!(f.native.bytes(first.1, 0, 1), b"B");
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let last = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        assert_eq!(f.native.bytes(last.1, 0, 1), [b'B' + 31]);
        check("active-32-retained-generations-and-old-new-oracle");
        for pin in pins {
            f.workspace.releasedir(pin).unwrap();
        }
        let (released, released_packs) = physical_private_files(&private);
        assert_eq!(
            released,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        assert_eq!(released_packs, 0);
        assert!(released < retained);
        println!("STAGE_ALLOCATION case=retained32 retained_bytes={retained} released_bytes={released} retained_pack_pages={retained_packs} released_pack_pages={released_packs}");
        check("active-32-pins-release-exact-blocks");
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-32-generations-exact-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_mixed_compact() {
        let f = Fixture::new_fresh(Gate::None);
        let root = f.workspace.root().serial;
        for index in 0..128u8 {
            let name = format!("c{index:03}");
            let (file, handle) = f
                .workspace
                .create_file(
                    root,
                    name.as_bytes(),
                    FileCreateOptions {
                        mode: 0o644,
                        umask: 0,
                        exclusive: true,
                        open: FileOpenOptions {
                            access: FileAccess::ReadWrite,
                            ..FileOpenOptions::default()
                        },
                    },
                    deadline(),
                )
                .unwrap();
            f.workspace
                .write_file(handle, 0, &f.own(b"A"), deadline())
                .unwrap();
            f.workspace.release(handle).unwrap();
            f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        }
        let first = f.workspace.stage(deadline()).unwrap();
        for index in (0..10).chain(std::iter::once(80)) {
            let name = format!("c{index:03}");
            let file = f.lookup(name.as_bytes());
            let handle = f
                .workspace
                .open_file(
                    file.serial,
                    FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                    ReferenceScope::Local,
                    deadline(),
                )
                .unwrap();
            f.workspace
                .write_file(handle, 1, &f.own(b"!"), deadline())
                .unwrap();
            f.workspace.release(handle).unwrap();
            f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        }
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let (before, before_packs) = physical_private_files(&private);
        assert_eq!(
            before,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        assert_eq!(before_packs, 3);
        let counts_before = f.workspace.backing_status().unwrap();
        let started = Instant::now();
        f.workspace.commit_staged(&first, deadline()).unwrap();
        let commit_wall = started.elapsed();
        let counts_after = f.workspace.backing_status().unwrap();
        println!("STAGE_PHASE case=mixed128 commit_staged_wall_ns={} pack_fetches={} index_fetches={} pack_page_writes={} index_page_writes={} cache_claim=none", commit_wall.as_nanos(), counts_after.active_pack_fetches - counts_before.active_pack_fetches, counts_after.active_index_fetches - counts_before.active_index_fetches, counts_after.active_pack_page_writes - counts_before.active_pack_page_writes, counts_after.active_index_page_writes - counts_before.active_index_page_writes);
        let (middle, middle_packs) = physical_private_files(&private);
        assert_eq!(
            middle,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        assert_eq!(middle_packs, 2);
        let g1 = attr(f.native.attributes(first.stage().candidate_root, b"c000"));
        assert_eq!(f.native.bytes(g1.1, 0, 1), b"A");
        let live = f.lookup(b"c000");
        let handle = f
            .workspace
            .open(live.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.read(handle, 0, 2), b"A!");
        f.workspace.release(handle).unwrap();
        f.workspace.forget(live.serial, 1, ReferenceScope::Local);
        println!("STAGE_ALLOCATION case=mixed128 before_bytes={before} middle_bytes={middle} before_pack_pages={before_packs} middle_pack_pages={middle_packs}");
        check("active-mixed-two-pages-compact-to-one-destination");
        f.workspace.commit(deadline()).unwrap();
        let (after, after_packs) = physical_private_files(&private);
        assert_eq!(after, f.workspace.backing_status().unwrap().allocated_bytes);
        assert_eq!(after_packs, 0);
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let g2 = attr(f.native.attributes(branch.effective_root, b"c000"));
        assert_eq!(f.native.bytes(g2.1, 0, 2), b"A!");
        let other = attr(f.native.attributes(branch.effective_root, b"c020"));
        assert_eq!(f.native.bytes(other.1, 0, 1), b"A");
        check("active-mixed-g1-g2-bytes-and-refund");
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-mixed-exact-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_mutation_compact() {
        let f = Fixture::new_fresh(Gate::None);
        let root = f.workspace.root().serial;
        for index in 0..128u8 {
            let name = format!("u{index:03}");
            let (file, handle) = f
                .workspace
                .create_file(
                    root,
                    name.as_bytes(),
                    FileCreateOptions {
                        mode: 0o644,
                        umask: 0,
                        exclusive: true,
                        open: FileOpenOptions {
                            access: FileAccess::ReadWrite,
                            ..FileOpenOptions::default()
                        },
                    },
                    deadline(),
                )
                .unwrap();
            f.workspace
                .write_file(handle, 0, &f.own(b"A"), deadline())
                .unwrap();
            f.workspace.release(handle).unwrap();
            f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        }
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let old = private_pack_files(&private);
        assert_eq!(old.len(), 2);
        for index in 0..45u8 {
            let name = format!("u{index:03}");
            let file = f.lookup(name.as_bytes());
            let handle = f
                .workspace
                .open_file(
                    file.serial,
                    FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                    ReferenceScope::Local,
                    deadline(),
                )
                .unwrap();
            f.workspace
                .write_file(handle, 0, &f.own(b"B"), deadline())
                .unwrap();
            f.workspace.release(handle).unwrap();
            f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        }
        let current = private_pack_files(&private);
        assert!(!current.contains(&old[0]));
        assert_eq!(
            physical_private_files(&private).0,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        let survivor = f.lookup(b"u079");
        let handle = f
            .workspace
            .open(survivor.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.read(handle, 0, 1), b"A");
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(survivor.serial, 1, ReferenceScope::Local);
        println!(
            "STAGE_ALLOCATION case=mutation_compact old_pack_pages={} current_pack_pages={}",
            old.len(),
            current.len()
        );
        check("active-mutation-compacts-mixed-sealed-page");
        f.workspace.commit(deadline()).unwrap();
        assert_eq!(physical_private_files(&private).1, 0);
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-mutation-compaction-commit-and-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_payload_refund() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let data = vec![b'Z'; 8192];
        let payload = f.own(&data);
        f.workspace
            .write_file(handle, 0, &payload, deadline())
            .unwrap();
        drop(payload);
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let before = private_files_with_prefix(&private, "p-");
        assert_eq!(before.len(), 1);
        check("active-large-payload-owned-before-commit");
        f.workspace.commit(deadline()).unwrap();
        let after = private_files_with_prefix(&private, "p-");
        assert!(after.is_empty());
        assert_eq!(
            physical_private_files(&private).0,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 0, data.len()), data);
        println!(
            "STAGE_ALLOCATION case=payload_refund before_files={} after_files={}",
            before.len(),
            after.len()
        );
        check("active-large-payload-refund-and-byte-oracle");
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-large-payload-exact-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_quota_refusal() {
        let f = Fixture::with_quota(Gate::None, 2 * 1024 * 1024);
        f.edit(b"data.bin", 10, 11, b"A");
        let file = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let input = f.own(b"B");
        let before = f.workspace.backing_status().unwrap();
        let available = before.quota_bytes - before.allocated_bytes - before.reserved_bytes;
        let mut blocks = available / 4096;
        while blocks + blocks.div_ceil(256) > available / 4096 {
            blocks -= 1;
        }
        let spare = f.own(&vec![0xcc; blocks as usize * 4096]);
        let full = f.workspace.backing_status().unwrap();
        assert!(full.quota_bytes - full.allocated_bytes - full.reserved_bytes <= 4096);
        let revision = f.workspace.status().unwrap().revision;
        let error = f
            .workspace
            .write_file(handle, 10, &input, deadline())
            .unwrap_err();
        assert!(matches!(
            error,
            WorkspaceError::Backing(_) | WorkspaceError::Capacity
        ));
        assert_eq!(f.workspace.status().unwrap().revision, revision);
        assert_eq!(f.read(handle, 10, 1), b"A");
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        assert_eq!(
            physical_private_files(&private).0,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        println!("STAGE_ALLOCATION case=quota_refusal full_bytes={} remaining_bytes={} refusal={error:?}", full.allocated_bytes, full.quota_bytes - full.allocated_bytes - full.reserved_bytes);
        check("active-quota-refusal-keeps-acknowledged-bytes-and-charge");
        drop(spare);
        drop(input);
        f.workspace.reclaim_payloads(deadline()).unwrap();
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 2, ReferenceScope::Local);
        f.workspace.commit(deadline()).unwrap();
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-quota-refusal-exact-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_separated4096() {
        let f = Fixture::new_fresh(Gate::None);
        let (file, handle) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"separated",
                FileCreateOptions {
                    mode: 0o644,
                    umask: 0,
                    exclusive: true,
                    open: FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                },
                deadline(),
            )
            .unwrap();
        let mut oracle = vec![b'A'; 8194];
        f.workspace
            .write_file(handle, 0, &f.own(&oracle), deadline())
            .unwrap();
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        f.workspace.commit(deadline()).unwrap();
        let file = f.lookup(b"separated");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        for index in 0..4096usize {
            let byte = b'B' + (index % 24) as u8;
            f.workspace
                .write_file(handle, (index * 2) as u64, &f.own(&[byte]), deadline())
                .unwrap();
            oracle[index * 2] = byte;
        }
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let (before, packs_before) = physical_private_files(&private);
        assert_eq!(
            before,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        assert!(
            before <= 3 * 1024 * 1024,
            "one-file 3 MiB design bound: {before}"
        );
        check("active-4096-separated-full-private-backing-bound");
        let counts_before = f.workspace.backing_status().unwrap();
        let started = Instant::now();
        f.workspace
            .commit(Instant::now() + Duration::from_secs(25))
            .unwrap();
        let commit_wall = started.elapsed();
        let counts_after = f.workspace.backing_status().unwrap();
        println!("STAGE_PHASE case=separated4096 commit_wall_ns={} pack_fetches={} index_fetches={} cache_claim=none", commit_wall.as_nanos(), counts_after.active_pack_fetches - counts_before.active_pack_fetches, counts_after.active_index_fetches - counts_before.active_index_fetches);
        let (after, packs_after) = physical_private_files(&private);
        assert_eq!(after, counts_after.allocated_bytes);
        assert_eq!(packs_after, 0);
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"separated"));
        assert_eq!(f.native.bytes(saved.1, 0, oracle.len()), oracle);
        println!("STAGE_ALLOCATION case=separated4096 before_bytes={before} after_bytes={after} before_pack_pages={packs_before} after_pack_pages={packs_after}");
        check("active-4096-separated-commit-and-full-byte-oracle");
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-4096-separated-exact-clean-close");
    }
    fn source_grouping(count: u64, max_reads: u64) {
        // Nonadjacent edits, emitted in file order but packed in write order.
        // An index lookup/pack load per edit fails the count oracle even if
        // elapsed wall happens to be low. This remains a public Workspace test.
        let f = Fixture::new_fresh(Gate::None);
        let (file, handle) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"grouped",
                FileCreateOptions {
                    mode: 0o644,
                    umask: 0,
                    exclusive: true,
                    open: FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                },
                deadline(),
            )
            .unwrap();
        let mut expected = vec![b'A'; 8194];
        f.workspace
            .write_file(handle, 0, &f.own(&expected), deadline())
            .unwrap();
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        f.workspace.commit(deadline()).unwrap();
        let file = f.lookup(b"grouped");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let mut positions = std::collections::BTreeSet::new();
        for i in 0..count {
            let offset = (104729 + i * 2654435761) % expected.len() as u64;
            assert!(positions.insert(offset));
            let byte = b'B' + (i % 24) as u8;
            f.workspace
                .write_file(handle, offset, &f.own(&[byte]), deadline())
                .unwrap();
            expected[offset as usize] = byte;
        }
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        let before = f.workspace.backing_status().unwrap();
        let commit_started = Instant::now();
        f.workspace.commit(deadline()).unwrap();
        let commit_wall_ns = commit_started.elapsed().as_nanos();
        let after = f.workspace.backing_status().unwrap();
        let reads = after.active_pack_fetches - before.active_pack_fetches;
        let seeks = after.active_index_seeks - before.active_index_seeks;
        assert!(reads > 0 && reads <= max_reads, "pack reads: {reads}");
        println!("SOURCE_GROUPING case=dispersed{count} commit_wall_ns={commit_wall_ns} pack_reads={reads} index_seeks={seeks} ref_limit=256 byte_limit=32768");
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"grouped"));
        assert_eq!(f.native.bytes(saved.1, 0, expected.len()), expected);
        check(&format!(
            "active-source-grouped-{count}-count-and-full-bytes"
        ));
        f.workspace.close_clean().unwrap();
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        assert_eq!(physical_private_files(&private), (0, 0));
        check(&format!("active-source-grouped-{count}-clean-close-refund"));
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_source_grouping_100() {
        source_grouping(100, 8);
    }

    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_source_grouping() {
        source_grouping(512, 32);
    }

    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_source_grouping_4097() {
        source_grouping(4097, 1024);
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_cleanup_failure() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        f.workspace
            .write_file(handle, 0, &f.own(b"A"), deadline())
            .unwrap();
        let private =
            std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()).to_path_buf();
        let old = private_pack_files(&private);
        assert_eq!(old.len(), 1);
        let second = f.own(b"B");
        let old_path = private_file_path(&private, &old[0]);
        // Keep the old allocated file, but make its registered relative path
        // a different identity. Retirement must refuse rather than refund or
        // adopt that replacement, even though the new selected bytes are valid.
        std::fs::rename(&old_path, old_path.with_extension("retained")).unwrap();
        std::fs::create_dir(&old_path).unwrap();
        let error = f
            .workspace
            .write_file(handle, 0, &second, deadline())
            .unwrap_err();
        let WorkspaceError::Published {
            receipt,
            published_handle,
            cause,
        } = error
        else {
            panic!("published failure must retain receipt: {error:?}")
        };
        assert_eq!(receipt.accepted_bytes, 1);
        assert_eq!(receipt.inode, file.serial);
        assert_eq!(published_handle, None);
        assert!(matches!(*cause, WorkspaceError::Io));
        assert_eq!(f.read(handle, 0, 1), b"B");
        println!("STAGE_FAILURE case=cleanup_failure injection=physical-path-identity-conflict accepted_bytes={} cause={cause:?}", receipt.accepted_bytes);
        assert!(
            f.workspace
                .backing_status()
                .unwrap()
                .active_retired_pack_pages
                > 0
        );
        assert_eq!(
            physical_private_files(&private).0,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        check("active-postpublication-cleanup-failure-keeps-receipt-and-new-bytes");
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        check("active-failed-cleanup-retains-charged-custody");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_split_slot() {
        let f = Fixture::new(Gate::None);
        let file = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        f.workspace
            .write_file(handle, 100, &f.own(&[b'X'; 128]), deadline())
            .unwrap();
        f.workspace
            .write_file(handle, 104, &f.own(b"YYYY"), deadline())
            .unwrap();
        f.workspace.release(handle).unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        let mut expected: Vec<u8> = (0..256).map(|index| (index % 251) as u8).collect();
        expected[100..228].fill(b'X');
        expected[104..108].fill(b'Y');
        let before = f.workspace.backing_status().unwrap();
        f.workspace.commit(deadline()).unwrap();
        let after = f.workspace.backing_status().unwrap();
        assert!(after.active_pack_fetches > before.active_pack_fetches);
        let Response::History(result) = f.branch() else {
            panic!("branch result")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("branch snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 0, expected.len()), expected);
        check("active-split-packed-slot-final-byte-oracle");
        f.workspace.close_clean().unwrap();
        check("active-split-packed-slot-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_quick_controls() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                data.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        for index in 0..4097usize {
            let byte = b'B' + (index % 24) as u8;
            f.workspace
                .write_file(handle, (index * 2) as u64, &f.own(&[byte]), deadline())
                .unwrap();
        }
        f.workspace.release(handle).unwrap();
        f.workspace.forget(data.serial, 1, ReferenceScope::Local);
        let pinned = f
            .workspace
            .opendir(f.workspace.root().serial, ReferenceScope::Local)
            .unwrap();
        f.workspace
            .commit(Instant::now() + Duration::from_secs(25))
            .unwrap();
        let held = f.workspace.backing_status().unwrap();
        assert!(held.active_pack_pages >= 52);
        let clean_started = Instant::now();
        f.workspace.commit(deadline()).unwrap();
        let clean_wall = clean_started.elapsed();
        let clean = f.workspace.backing_status().unwrap();
        assert_eq!(clean.active_pack_fetches - held.active_pack_fetches, 0);
        assert!(clean.active_index_fetches - held.active_index_fetches < 128);
        println!("STAGE_PHASE case=clean_control commit_wall_ns={} pack_fetches={} index_fetches={} retained_pack_pages={} cache_claim=none", clean_wall.as_nanos(), clean.active_pack_fetches - held.active_pack_fetches, clean.active_index_fetches - held.active_index_fetches, clean.active_pack_pages);
        check("active-clean-commit-skips-retained-old-journal");
        let other = f.lookup(b"other.bin");
        let handle = f
            .workspace
            .open_file(
                other.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        f.workspace
            .write_file(handle, 0, &f.own(b"Q"), deadline())
            .unwrap();
        f.workspace.release(handle).unwrap();
        f.workspace.forget(other.serial, 1, ReferenceScope::Local);
        let before = f.workspace.backing_status().unwrap();
        let edit_started = Instant::now();
        f.workspace.commit(deadline()).unwrap();
        let edit_wall = edit_started.elapsed();
        let after = f.workspace.backing_status().unwrap();
        assert!(after.active_pack_fetches - before.active_pack_fetches <= 2);
        assert!(after.active_index_fetches - before.active_index_fetches < 256);
        println!("STAGE_PHASE case=one_edit_control commit_wall_ns={} pack_fetches={} index_fetches={} retained_pack_pages={} cache_claim=none", edit_wall.as_nanos(), after.active_pack_fetches - before.active_pack_fetches, after.active_index_fetches - before.active_index_fetches, after.active_pack_pages);
        check("active-one-edit-commit-skips-retained-old-journal");
        f.workspace.releasedir(pinned).unwrap();
        assert_eq!(f.workspace.backing_status().unwrap().active_pack_pages, 0);
        f.workspace.close_clean().unwrap();
        check("active-quick-controls-refund-and-clean-close");
    }
    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_semantics() {
        let f = Fixture::new(Gate::Delivery);
        let branch = f.branch();
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::InvalidInput)
        ));
        assert_eq!(f.workspace.status().unwrap().generation, 1);
        let readonly = f
            .host
            .attach(
                Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        assert!(matches!(
            readonly.stage(deadline()),
            Err(WorkspaceError::ReadOnly)
        ));
        readonly.close_clean().unwrap();
        let data = f.lookup(b"data.bin");
        let alias = f.lookup(b"alias");
        assert_eq!(data.serial, alias.serial);
        f.lookup(b"other.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        let captured = f.edit(b"data.bin", 10, 14, b"GGGG");
        f.edit(b"other.bin", 8, 10, b"OO");
        let captured_attr = f.workspace.getattr(data.serial).unwrap();
        let second = f
            .host
            .attach(
                Fixture::options("second", 33, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        second
            .lookup(
                second.root().serial,
                b"data.bin",
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        let payload = second.own_payload(1, &mut &b"B"[..], deadline()).unwrap();
        let second_handle = second
            .open_file(
                data.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        second
            .write_file(second_handle, 0, &payload, deadline())
            .unwrap();
        second.release(second_handle).unwrap();
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        assert!(matches!(
            second.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(
            f.workspace.status().unwrap().generation,
            captured.generation + 1
        );
        let successor = f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(successor.generation, captured.generation + 1);
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        check("capture-live-successor-and-consumer-admission");
        f.native.release();
        let selector = saving.join().unwrap().unwrap();
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(saved.0, data.serial);
        assert_eq!(saved.2, data.size);
        assert_eq!(
            (saved.3, saved.4),
            (captured_attr.mtime_seconds, captured_attr.mtime_nanoseconds)
        );
        assert_eq!(
            f.native.bytes(saved.1, 8, 8),
            [8, 9, b'G', b'G', b'G', b'G', 14, 15]
        );
        assert_eq!(
            attr(
                f.native
                    .attributes(selector.stage().candidate_root, b"alias")
            ),
            saved
        );
        let other = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"other.bin"),
        );
        assert_eq!(f.native.bytes(other.1, 8, 2), b"OO");
        assert_eq!(f.branch(), branch);
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        f.edit(b"alias", 11, 13, b"x");
        assert_eq!(f.read(handle, 10, 3), b"LxE");
        f.workspace.reclaim_metadata(deadline()).unwrap();
        f.workspace.reclaim_payloads(deadline()).unwrap();
        assert_eq!(f.read(handle, 10, 3), b"LxE");
        f.counts(2);
        check("frozen-bytes-attributes-aliases-and-one-tree-save");
        stage_retained(&f, selector);
        f.workspace.release(handle).unwrap();
    }

    fn native_save(kill: bool) {
        let f = Fixture::new(Gate::NativeSave);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for byte in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *byte = random as u8;
        }
        f.edit(b"data.bin", 0, bytes.len() as u64, &bytes);
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let result = saving.join().unwrap();
        if kill {
            let error = result.unwrap_err();
            let WorkspaceError::Stage(failure) = error else {
                panic!("lost save must retain capture: {error:?}")
            };
            assert_eq!(failure.disposition, StageFailureDisposition::Unknown);
            assert_eq!(failure.phase, StagePhase::FileSave);
            assert!(matches!(&failure.cause,WorkspaceError::Service(failure) if failure.unknown));
            assert_eq!(
                f.workspace.status().unwrap().submission.unwrap().phase,
                StagePhase::Failed
            );
            assert!(matches!(
                f.workspace.stage(deadline()),
                Err(WorkspaceError::Busy)
            ));
            f.edit(b"data.bin", 10, 14, b"NEXT");
            assert_eq!(f.read(handle, 10, 4), b"NEXT");
            assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
            println!("STAGE_FAILURE {failure:?}");
            check("unknown-save-retains-G-and-live-successor");
        } else {
            let selector = result.unwrap();
            let saved = attr(
                f.native
                    .attributes(selector.stage().candidate_root, b"data.bin"),
            );
            assert_eq!(
                f.native.bytes(saved.1, 0, MAX_READ_BYTES),
                bytes[..MAX_READ_BYTES]
            );
            assert_eq!(f.read(handle, 10, 4), b"LIVE");
            assert_eq!(f.branch(), branch);
            f.counts(1);
            check("actual-service-save-with-live-successor-progress");
            stage_retained(&f, selector);
        }
    }
    #[test]
    #[ignore = "requires stage_route.py native SQLite save observation"]
    fn stage_native_save() {
        native_save(false);
    }
    #[test]
    #[ignore = "requires stage_route.py native SQLite save loss"]
    fn stage_unknown_save() {
        native_save(true);
    }

    #[test]
    #[ignore = "requires stage_route.py with metadata grant denied"]
    fn stage_metadata_denied() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let error = f.workspace.stage(deadline()).unwrap_err();
        let WorkspaceError::Stage(failure) = error else {
            panic!("capture failure not retained: {error:?}")
        };
        assert_eq!(failure.phase, StagePhase::MetadataSave);
        assert_eq!(
            failure.disposition,
            StageFailureDisposition::KnownBeforeStage
        );
        assert!(
            matches!(&failure.cause,WorkspaceError::Service(failure) if failure.code==Code::Denied && !failure.unknown)
        );
        let pending = failure.pending.unwrap();
        assert_eq!(pending.serial, data.serial);
        assert!(pending.metadata.is_none());
        assert_eq!(f.native.bytes(pending.content, 10, 4), b"GGGG");
        assert!(failure.source_failure.is_none());
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.saved_files, 1);
        assert_eq!(status.saved_metadata, 0);
        assert_eq!(status.stage_token, None);
        assert_eq!(f.branch(), branch);
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        let observations = f.native.observations.lock().unwrap();
        assert!(!observations
            .operations
            .iter()
            .any(|op| matches!(op, Operation::HistoryCommand(_))));
        println!("STAGE_FAILURE {failure:?}");
        check("known-metadata-denial-retains-saved-file-G-and-D1");
    }
    #[test]
    #[ignore = "requires stage_route.py and native service"]
    fn stage_lowering() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let mut oracle: Vec<u8> = (0..2048).map(|i| (i % 251) as u8).collect();
        for (start, end, bytes) in [
            (100, 110, b"abc".as_slice()),
            (200, 200, b"12345".as_slice()),
            (500, 700, b"".as_slice()),
        ] {
            f.edit(b"data.bin", start, end, bytes);
            oracle.splice(start as usize..end as usize, bytes.iter().copied());
        }
        let selector = f.workspace.stage(deadline()).unwrap();
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(saved.2, data.size - 202);
        assert_eq!(f.native.bytes(saved.1, 0, 1024), oracle[..1024]);
        let offset = 32 * 1024 * 1024u64;
        let distant: Vec<_> = (offset + 202..offset + 202 + 512)
            .map(|i| (i % 251) as u8)
            .collect();
        assert_eq!(f.native.bytes(saved.1, offset, 512), distant);
        assert_eq!(f.branch(), branch);
        f.counts(1);
        let observed = f.native.observations.lock().unwrap();
        let replacement = observed
            .operations
            .iter()
            .find_map(|op| {
                if let Operation::SaveFile {
                    base: Some(_),
                    replacement,
                    ..
                } = op
                {
                    Some(*replacement)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(replacement, 8);
        drop(observed);
        check("normalized-splice-lowering-and-streamed-exact-input");
        stage_retained(&f, selector);
    }

    #[test]
    #[ignore = "requires stage_route.py and an independent Branch writer"]
    fn stage_head_moved() {
        let f = Fixture::new(Gate::Delivery);
        let data = f.lookup(b"data.bin");
        let Response::History(before) = f.branch() else {
            panic!("missing branch")
        };
        let HistoryResult::BranchSnapshot(before) = *before else {
            panic!("missing snapshot")
        };
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        // The update this request declares: one directory row that drops the old
        // alias and binds the new one, and no typed identity of its own.
        let directories = [DirectoryChange {
            parent: before.root_serial.unwrap(),
            changes: vec![
                (b"alias".to_vec(), None),
                (b"new-alias".to_vec(), Some(data.serial)),
            ],
        }];
        let identities: [PreparedIdentity; 0] = [];
        let body = prepared_stream::body(&directories, &identities).unwrap();
        let response = f
            .native
            .request_with_body(
                Operation::HistoryCommand(HistoryCommand::Commit(prepared_stream::header(
                    &before,
                    [44; 32],
                    1,
                    &directories,
                    &identities,
                ))),
                &body,
                16384,
                &mut std::io::sink(),
            )
            .unwrap();
        let Response::History(response) = response else {
            panic!("missing Commit")
        };
        let HistoryResult::Committed(CommitOutcomeWire::Committed(committed)) = *response else {
            panic!("Commit did not advance")
        };
        f.native.release();
        let error = saving.join().unwrap().unwrap_err();
        let WorkspaceError::Stage(failure) = error else {
            panic!("missing retained failure: {error:?}")
        };
        assert_eq!(failure.phase, StagePhase::StageChanges);
        assert_eq!(
            failure.disposition,
            StageFailureDisposition::KnownBeforeStage
        );
        let WorkspaceError::Service(cause) = &failure.cause else {
            panic!("missing service conflict")
        };
        assert_eq!(cause.code, Code::HeadMoved);
        assert!(!cause.unknown);
        let context = cause.history.as_ref().unwrap();
        assert_eq!(
            context.conflict,
            Some(HistoryConflict::BranchMoved {
                expected_head: before.branch.head_commit,
                actual_head: Some(committed.commit),
                expected_base: before.branch.base_layer,
                actual_base: before.branch.base_layer
            })
        );
        assert!(matches!(
            context.stage,
            StageObservation::Unobserved | StageObservation::Absent(_)
        ));
        assert!(failure.observed_stage.is_none());
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        f.edit(b"data.bin", 10, 14, b"LIVE");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        let query = f
            .native
            .request(
                Operation::HistoryQuery(HistoryQuery::GetStage {
                    workspace: [31; 32],
                }),
                16384,
                &mut std::io::sink(),
            )
            .unwrap_err();
        assert_eq!(query.code, Code::NotFound);
        assert!(!query.unknown);
        println!("STAGE_FAILURE {failure:?}");
        check("stale-captured-head-preserves-exact-conflict-and-live-state");
    }
    #[test]
    #[ignore = "requires stage_route.py wide live fixture"]
    fn stage_frontier() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let mut names = vec![b"data.bin".to_vec(), b"other.bin".to_vec()];
        names.extend((0..102).map(|i| format!("f{i:03}").into_bytes()));
        let mut serials = Vec::new();
        for (index, name) in names.iter().enumerate() {
            serials.push(f.lookup(name).serial);
            f.edit(name, 10, 12, &[index as u8, 0xfe]);
            f.workspace.reclaim_metadata(deadline()).unwrap();
        }
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 104);
        let selector = f
            .workspace
            .stage(Instant::now() + Duration::from_secs(25))
            .unwrap();
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.saved_files, 104);
        assert_eq!(status.saved_metadata, 104);
        for (index, name) in names.iter().enumerate() {
            let saved = attr(f.native.attributes(selector.stage().candidate_root, name));
            assert_eq!(saved.0, serials[index]);
            assert_eq!(f.native.bytes(saved.1, 10, 2), [index as u8, 0xfe]);
        }
        assert_eq!(f.branch(), branch);
        f.counts(104);
        check("all-104-inode-completions-persist-and-stage");
        stage_retained(&f, selector);
    }
    struct RestoreFileLimit;
    impl Drop for RestoreFileLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires stage_route.py ignoring XFSZ for actual native failure"]
    fn stage_completion_failure() {
        let f = Fixture::new(Gate::CompletionFailure);
        let data = f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let restore = RestoreFileLimit;
        let result = f.workspace.stage(deadline());
        drop(restore);
        let WorkspaceError::Stage(failure) = result.unwrap_err() else {
            panic!("missing captured native failure")
        };
        assert!(matches!(failure.cause, WorkspaceError::Backing(_)));
        let observed = f.native.observations.lock().unwrap();
        assert_eq!(observed.saved_files.len(), 1);
        let root = observed.saved_files[0];
        let pending = failure.pending.unwrap();
        assert_eq!(pending.content, root);
        assert_eq!(pending.serial, data.serial);
        assert!(pending.metadata.is_some());
        assert_eq!(failure.phase, StagePhase::LocalBookkeeping);
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::UpdatePortableMetadata { .. }))
                .count(),
            1
        );
        assert!(!observed
            .operations
            .iter()
            .any(|op| matches!(op, Operation::HistoryCommand(_))));
        drop(observed);
        assert_eq!(f.native.bytes(root, 10, 4), b"GGGG");
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.phase, StagePhase::Failed);
        assert_eq!(status.saved_files, 1);
        assert_eq!(status.saved_metadata, 1);
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, data.size);
        println!("STAGE_FAILURE {failure:?}");
        println!(
            "STAGE_RESOURCE {:?} {:?}",
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
        check("known-file-save-survives-native-completion-publication-failure");
    }
    #[test]
    #[ignore = "requires stage_route.py live metadata-only save"]
    fn stage_metadata_only() {
        let f = Fixture::new(Gate::None);
        let branch = f.branch();
        let data = f.lookup(b"data.bin");
        let Response::History(snapshot) = branch.clone() else {
            panic!("missing branch")
        };
        let HistoryResult::BranchSnapshot(snapshot) = *snapshot else {
            panic!("missing snapshot")
        };
        let original = attr(f.native.attributes(snapshot.effective_root, b"data.bin"));
        f.edit(b"data.bin", 100, 100, b"");
        let captured = f.workspace.getattr(data.serial).unwrap();
        let selector = f.workspace.stage(deadline()).unwrap();
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(
            (saved.0, saved.1, saved.2),
            (original.0, original.1, original.2)
        );
        assert_eq!(
            (saved.3, saved.4),
            (captured.mtime_seconds, captured.mtime_nanoseconds)
        );
        assert_eq!(f.branch(), branch);
        let observed = f.native.observations.lock().unwrap();
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::SaveFile { base: Some(_), .. }))
                .count(),
            0
        );
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::UpdatePortableMetadata { .. }))
                .count(),
            1
        );
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(
                    op,
                    Operation::HistoryCommand(HistoryCommand::StageChanges(_))
                ))
                .count(),
            1
        );
        drop(observed);
        let status = f.workspace.status().unwrap().submission.unwrap();
        assert_eq!(status.saved_files, 0);
        assert_eq!(status.saved_metadata, 1);
        check("metadata-only-stage-preserves-root-without-file-save");
        stage_retained(&f, selector);
    }
    #[test]
    #[ignore = "requires stage_route.py with ordinary quota occupied"]
    fn stage_headroom() {
        let f = Fixture::with_quota(Gate::None, 2 * 1024 * 1024);
        let data = f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let before = f.workspace.backing_status().unwrap();
        let available = before.quota_bytes - before.allocated_bytes - before.reserved_bytes;
        let mut blocks = available / 4096;
        while blocks + blocks.div_ceil(256) > available / 4096 {
            blocks -= 1;
        }
        let spare = f.own(&vec![0xcc; blocks as usize * 4096]);
        let full = f.workspace.backing_status().unwrap();
        assert!(full.quota_bytes - full.allocated_bytes - full.reserved_bytes <= 4096);
        let selector = f.workspace.stage(deadline()).unwrap();
        let after = f.workspace.backing_status().unwrap();
        assert!(after.allocated_bytes > full.allocated_bytes);
        assert!(after.reserved_bytes < full.reserved_bytes);
        assert_eq!(
            after.allocated_bytes + after.reserved_bytes,
            full.allocated_bytes + full.reserved_bytes
        );
        let saved = attr(
            f.native
                .attributes(selector.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(saved.0, data.serial);
        assert_eq!(f.native.bytes(saved.1, 10, 4), b"GGGG");
        println!("STAGE_HEADROOM before={before:?} full={full:?} after={after:?} unrelated_owned_bytes={}",spare.len());
        check("reserved-stage-progress-with-ordinary-disk-quota-occupied");
        stage_retained(&f, selector);
    }
    fn writable(f: &Fixture, serial: u64) -> HandleId {
        f.workspace
            .open_file(
                serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    ..FileOpenOptions::default()
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap()
    }

    fn complete_read(f: &Fixture, handle: HandleId, length: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(length);
        while bytes.len() < length {
            let count = (length - bytes.len()).min(MAX_READ_BYTES);
            let next = f.read(handle, bytes.len() as u64, count);
            assert_eq!(next.len(), count);
            bytes.extend(next);
        }
        bytes
    }

    #[test]
    #[ignore = "requires stage_route.py and a live native service"]
    fn stage_active_hot_publication() {
        let f = Fixture::new_fresh(Gate::None);
        let file = f.lookup(b"data.bin");
        let other = f.lookup(b"other.bin");
        let alias = f.lookup(b"alias");
        let Response::History(old_branch) = f.branch() else {
            panic!("old branch")
        };
        let HistoryResult::BranchSnapshot(old_branch) = *old_branch else {
            panic!("old snapshot")
        };
        let old_content = attr(f.native.attributes(old_branch.effective_root, b"data.bin")).1;
        assert_eq!(alias.serial, file.serial);
        let held = writable(&f, file.serial);
        let inherited = writable(&f, other.serial);
        let mut first_oracle = complete_read(&f, held, file.size as usize);
        let old_bytes = first_oracle.clone();
        let mut second_oracle = complete_read(&f, inherited, other.size as usize);
        let mut ordinary = 0;
        let before = f.workspace.backing_status().unwrap();
        for index in 0..512usize {
            for (handle, offset, byte) in [
                (held, file.size + index as u64, b'B' + (index % 24) as u8),
                (inherited, (index * 2) as u64, b'Z' - (index % 24) as u8),
            ] {
                let old = f.workspace.backing_status().unwrap();
                f.workspace
                    .write_file(handle, offset, &f.own(&[byte]), deadline())
                    .unwrap();
                let new = f.workspace.backing_status().unwrap();
                if new.active_hot_writes > old.active_hot_writes
                    && new.active_hot_carries == old.active_hot_carries
                {
                    assert_eq!(new.active_index_seeks, old.active_index_seeks);
                    assert_eq!(new.active_index_fetches, old.active_index_fetches);
                    assert!(new.active_index_page_writes - old.active_index_page_writes + 1 <= 7);
                    ordinary += 1;
                }
            }
            first_oracle.push(b'B' + (index % 24) as u8);
            second_oracle[index * 2] = b'Z' - (index % 24) as u8;
        }
        let after = f.workspace.backing_status().unwrap();
        let hot = after.active_hot_writes - before.active_hot_writes;
        let carries = after.active_hot_carries - before.active_hot_carries;
        let admissions = after.active_hot_cursor_admissions - before.active_hot_cursor_admissions;
        assert_eq!(admissions, 2);
        assert_eq!(
            hot + admissions + 1,
            1024,
            "one empty root plus two admitted inodes"
        );
        assert!(ordinary > 0 && ordinary + carries >= hot);
        assert!(after.active_hot_admissions - before.active_hot_admissions < 16);
        assert!(
            after.active_hot_nodes <= 64
                && after.active_hot_cursors <= 8
                && after.active_hot_reserved_bytes <= 1 << 20
        );
        assert_eq!(complete_read(&f, held, first_oracle.len()), first_oracle);
        let alias_handle = writable(&f, alias.serial);
        assert_eq!(
            complete_read(&f, alias_handle, first_oracle.len()),
            first_oracle
        );
        f.workspace.release(alias_handle).unwrap();
        let private = std::path::PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap());
        assert!(!private_files_with_prefix(&private, "a-hot-v2").is_empty());
        assert_eq!(physical_private_files(&private).0, after.allocated_bytes);
        println!("STAGE_HOT ordinary={ordinary} hot_writes={} admissions={} carries={} seeks={} visits={} index_writes={} directory_writes={} pack_writes={} retirement_inspections={} legacy_routine_scans={} legacy_lookup_scans={} hot_reserved_bytes={} physical_bytes={}", after.active_hot_writes - before.active_hot_writes, after.active_hot_admissions - before.active_hot_admissions, after.active_hot_carries - before.active_hot_carries, after.active_index_seeks - before.active_index_seeks, after.active_index_node_visits - before.active_index_node_visits, after.active_index_page_writes - before.active_index_page_writes, after.active_directory_page_writes - before.active_directory_page_writes, after.active_pack_page_writes - before.active_pack_page_writes, after.active_retirement_inspections - before.active_retirement_inspections, after.routine_scans - before.routine_scans, after.lookup_scans - before.lookup_scans, after.active_hot_reserved_bytes, physical_private_files(&private).0);
        check("active-hot-publication-counts-and-alias-byte-oracle");
        let staged = f.workspace.stage(deadline()).unwrap();
        let saved = attr(
            f.native
                .attributes(staged.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(f.native.bytes(saved.1, 0, first_oracle.len()), first_oracle);
        f.workspace
            .write_file(held, first_oracle.len() as u64, &f.own(b"G"), deadline())
            .unwrap();
        first_oracle.push(b'G');
        let (unrelated, unheld) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"g2-hot",
                FileCreateOptions {
                    mode: 0o644,
                    umask: 0,
                    exclusive: true,
                    open: FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                },
                deadline(),
            )
            .unwrap();
        for offset in 0..4 {
            f.workspace
                .write_file(unheld, offset, &f.own(b"u"), deadline())
                .unwrap();
        }
        f.workspace.commit_staged(&staged, deadline()).unwrap();
        assert_eq!(complete_read(&f, held, first_oracle.len()), first_oracle);
        assert_eq!(
            f.native.bytes(saved.1, 0, first_oracle.len() - 1),
            &first_oracle[..first_oracle.len() - 1]
        );
        let before_unrelated = f.workspace.backing_status().unwrap();
        f.workspace
            .write_file(unheld, 4, &f.own(b"u"), deadline())
            .unwrap();
        let after_unrelated = f.workspace.backing_status().unwrap();
        assert_eq!(
            after_unrelated.active_hot_writes,
            before_unrelated.active_hot_writes + 1
        );
        assert_eq!(
            after_unrelated.active_index_seeks,
            before_unrelated.active_index_seeks
        );
        assert_eq!(
            after_unrelated.active_hot_normalizations,
            before_unrelated.active_hot_normalizations
        );
        for index in 0..64 {
            f.workspace
                .write_file(
                    held,
                    first_oracle.len() as u64,
                    &f.own(&[b'c' + (index % 10) as u8]),
                    deadline(),
                )
                .unwrap();
            first_oracle.push(b'c' + (index % 10) as u8);
        }
        assert_eq!(complete_read(&f, held, first_oracle.len()), first_oracle);
        assert_eq!(f.workspace.getattr(unrelated.serial).unwrap().size, 5);
        check("active-hot-g1-g2-and-post-commit-continuation");
        f.workspace
            .write_file(held, 5, &f.own(&[b'Z'; 129]), deadline())
            .unwrap();
        first_oracle[5..134].fill(b'Z');
        f.workspace.set_len(file.serial, 200, deadline()).unwrap();
        first_oracle.truncate(200);
        f.workspace.set_len(file.serial, 260, deadline()).unwrap();
        first_oracle.resize(260, 0);
        assert_eq!(complete_read(&f, held, first_oracle.len()), first_oracle);
        f.workspace
            .rename(
                f.workspace.root().serial,
                b"data.bin",
                f.workspace.root().serial,
                b"moved-hot",
                RenameFlags::default(),
                deadline(),
            )
            .unwrap();
        f.workspace
            .unlink(f.workspace.root().serial, b"alias", deadline())
            .unwrap();
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        assert_eq!(complete_read(&f, held, first_oracle.len()), first_oracle);
        let (_, orphan) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"orphan-hot",
                FileCreateOptions {
                    mode: 0o644,
                    umask: 0,
                    exclusive: true,
                    open: FileOpenOptions {
                        access: FileAccess::ReadWrite,
                        ..FileOpenOptions::default()
                    },
                },
                deadline(),
            )
            .unwrap();
        for offset in 0..8 {
            f.workspace
                .write_file(orphan, offset, &f.own(b"o"), deadline())
                .unwrap();
        }
        f.workspace
            .unlink(f.workspace.root().serial, b"orphan-hot", deadline())
            .unwrap();
        f.workspace
            .write_file(orphan, 8, &f.own(b"!"), deadline())
            .unwrap();
        assert_eq!(f.read(orphan, 0, 9), b"oooooooo!");
        f.workspace.release(held).unwrap();
        f.workspace.release(inherited).unwrap();
        f.workspace.release(unheld).unwrap();
        f.workspace.commit(deadline()).unwrap();
        assert_eq!(f.read(orphan, 0, 9), b"oooooooo!");
        f.workspace
            .write_file(orphan, 9, &f.own(b"?"), deadline())
            .unwrap();
        f.workspace.commit(deadline()).unwrap();
        assert_eq!(f.read(orphan, 0, 10), b"oooooooo!?");
        f.workspace.release(orphan).unwrap();
        let Response::History(result) = f.branch() else {
            panic!("branch")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"moved-hot"));
        assert_eq!(f.native.bytes(saved.1, 0, first_oracle.len()), first_oracle);
        let other_saved = attr(f.native.attributes(branch.effective_root, b"other.bin"));
        assert_eq!(
            f.native.bytes(other_saved.1, 0, second_oracle.len()),
            second_oracle
        );
        assert_eq!(f.native.bytes(old_content, 0, old_bytes.len()), old_bytes);
        assert_eq!(
            physical_private_files(&private).0,
            f.workspace.backing_status().unwrap().allocated_bytes
        );
        f.workspace.forget(file.serial, 1, ReferenceScope::Local);
        f.workspace.forget(other.serial, 1, ReferenceScope::Local);
        f.workspace
            .forget(unrelated.serial, 1, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        assert_eq!(physical_private_files(&private), (0, 0));
        check("active-hot-exact-blocks-and-clean-close");
    }

    #[test]
    #[ignore = "child process for the mounted Commit continuity proof"]
    fn stage_hot_child() {
        use std::{
            io::BufRead,
            sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            },
        };
        let path = std::env::var("LAYERFS_HOT_CHILD_PATH").unwrap();
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .open(path)
            .unwrap();
        let inode = file.metadata().unwrap().ino();
        let pid = std::process::id();
        let stop = Arc::new(AtomicBool::new(false));
        let child_stop = stop.clone();
        let beat = std::thread::spawn(move || {
            let mut ticks = 0u64;
            while !child_stop.load(Ordering::Acquire) {
                println!("CHILD HEARTBEAT {pid} {inode} {ticks}");
                std::io::stdout().flush().unwrap();
                ticks += 1;
                std::thread::park_timeout(Duration::from_millis(20));
            }
        });
        println!("CHILD READY {pid} {inode}");
        std::io::stdout().flush().unwrap();
        for command in std::io::stdin().lock().lines() {
            let command = command.unwrap();
            if command == "q" {
                break;
            }
            let (tag, byte) = command.split_once(' ').unwrap();
            file.write_all(&[byte.parse::<u8>().unwrap()]).unwrap();
            assert_eq!(file.metadata().unwrap().ino(), inode);
            println!(
                "CHILD ACK {pid} {inode} {tag} {}",
                file.metadata().unwrap().len()
            );
            std::io::stdout().flush().unwrap();
        }
        stop.store(true, Ordering::Release);
        beat.thread().unpark();
        beat.join().unwrap();
        let mut bytes = [0; 5];
        file.read_exact_at(&mut bytes, file.metadata().unwrap().len() - 5)
            .unwrap();
        assert_eq!(&bytes, b"BCDEF");
        println!("CHILD CLOSED {pid} {inode}");
        std::io::stdout().flush().unwrap();
    }

    #[test]
    #[ignore = "requires privileged stage_route.py with /dev/fuse"]
    fn stage_active_hot_continuity() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command, Stdio},
        };
        let f = Fixture::new(Gate::Continuity);
        let original = f.lookup(b"data.bin");
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--nocapture",
                "--test-threads=1",
                "--exact",
                "linux::stage_hot_child",
            ])
            .env(
                "LAYERFS_HOT_CHILD_PATH",
                f.workspace.mount_path().join("data.bin"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let pid = child.id();
        let output = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let reading = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                tx.send(line.unwrap()).unwrap();
            }
        });
        let await_line = |wanted: &str| -> String {
            loop {
                let line = rx.recv_timeout(Duration::from_secs(5)).unwrap();
                if let Some(at) = line.find("CHILD ") {
                    println!("STAGE_PROCESS {}", &line[at..]);
                }
                if line.contains(wanted) {
                    return line;
                }
            }
        };
        let ready = await_line("CHILD READY ");
        assert!(ready.contains(&format!("{pid} {}", original.serial)));
        let mut input = child.stdin.take().unwrap();
        writeln!(input, "before1 66").unwrap();
        input.flush().unwrap();
        await_line("before1 ");
        writeln!(input, "before2 67").unwrap();
        input.flush().unwrap();
        await_line("before2 ");
        let workspace = f.workspace.clone();
        let committing = std::thread::spawn(move || workspace.commit(deadline()));
        f.native.wait_entered();
        await_line("CHILD HEARTBEAT ");
        writeln!(input, "during-save 68").unwrap();
        input.flush().unwrap();
        await_line("during-save ");
        assert_eq!(f.workspace.status().unwrap().generation, 2);
        f.native.release();
        f.native.wait_commit();
        await_line("CHILD HEARTBEAT ");
        writeln!(input, "during-c5 69").unwrap();
        input.flush().unwrap();
        await_line("during-c5 ");
        f.native.release_commit();
        let committed = committing.join().unwrap().unwrap();
        let root = match committed.outcome {
            CommitOutcomeWire::Committed(commit) => commit.root,
            CommitOutcomeWire::UpToDate { root, .. } => root,
        };
        let captured = attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(f.native.bytes(captured.1, original.size, 2), b"BC");
        check("active-mounted-process-g1-save-c5-and-g2-progress");
        writeln!(input, "after-commit 70").unwrap();
        input.flush().unwrap();
        await_line("after-commit ");
        writeln!(input, "q").unwrap();
        input.flush().unwrap();
        drop(input);
        await_line("CHILD CLOSED ");
        assert!(child.wait().unwrap().success());
        reading.join().unwrap();
        assert_eq!(
            std::fs::read(f.workspace.mount_path().join("data.bin")).unwrap()
                [original.size as usize..],
            *b"BCDEF"
        );
        mount.unmount(deadline()).unwrap();
        f.workspace.commit(deadline()).unwrap();
        let Response::History(result) = f.branch() else {
            panic!("branch")
        };
        let HistoryResult::BranchSnapshot(branch) = *result else {
            panic!("snapshot")
        };
        let saved = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        let alias = attr(f.native.attributes(branch.effective_root, b"alias"));
        assert_eq!(saved, alias);
        assert_eq!(f.native.bytes(saved.1, original.size, 5), b"BCDEF");
        f.workspace
            .forget(original.serial, 1, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("active-mounted-same-process-handle-post-commit-and-refund");
    }
}
