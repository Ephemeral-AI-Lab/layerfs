//! Actual mounted Linux STATE/EDIT route through the projected Workspace.
#[cfg(target_os = "linux")]
#[path = "../../layerfs-workspace/tests/support/native_workspace.rs"]
mod support;

#[cfg(target_os = "linux")]
mod linux {
    use super::support::{deadline, Fixture, Gate};
    use layerfs_workspace::ReferenceScope;
    use std::{
        fs::{self, File, OpenOptions},
        os::{
            fd::AsRawFd,
            unix::fs::{FileExt, MetadataExt},
        },
    };

    const STATE: u32 = 0xc058_f540;
    const EDIT: u32 = 0x5060_f541;

    fn state(file: &File) -> [u8; 88] {
        let mut bytes = [0; 88];
        bytes[..4].copy_from_slice(b"LFS2");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
        let result = unsafe { libc::ioctl(file.as_raw_fd(), STATE as _, bytes.as_mut_ptr()) };
        assert_eq!(result, 0, "STATE: {}", std::io::Error::last_os_error());
        assert_eq!(&bytes[..8], b"LFS2\x02\0\0\0");
        assert_eq!(&bytes[84..], &[0; 4]);
        bytes
    }

    fn number(bytes: &[u8], start: usize) -> u64 {
        u64::from_le_bytes(bytes[start..start + 8].try_into().unwrap())
    }

    fn read(file: &File, offset: u64, length: usize) -> Vec<u8> {
        let mut bytes = vec![0; length];
        let read = file.read_at(&mut bytes, offset).unwrap();
        bytes.truncate(read);
        bytes
    }

    fn edit(stamp: &[u8; 88], offset: u64, deleted: u64, replacement: &[u8]) -> [u8; 4192] {
        assert!(replacement.len() <= 4096);
        let mut bytes = [0; 4192];
        bytes[..4].copy_from_slice(b"LFE2");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[8..64].copy_from_slice(&stamp[8..64]);
        bytes[64..72].copy_from_slice(&offset.to_le_bytes());
        bytes[72..80].copy_from_slice(&deleted.to_le_bytes());
        bytes[80..84].copy_from_slice(&(replacement.len() as u32).to_le_bytes());
        bytes[96..96 + replacement.len()].copy_from_slice(replacement);
        bytes
    }

    fn call_edit(file: &File, bytes: &mut [u8; 4192]) -> Result<(), i32> {
        let result = unsafe { libc::ioctl(file.as_raw_fd(), EDIT as _, bytes.as_mut_ptr()) };
        if result == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().raw_os_error().unwrap())
        }
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and native service"]
    fn kernel_range_insert_commit() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        f.workspace.set_len(data.serial, 8192, deadline()).unwrap();
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.workspace.mount_path().join("data.bin"))
            .unwrap();
        let alias = File::open(f.workspace.mount_path().join("alias")).unwrap();
        let stamp = state(&file);
        assert_eq!(number(&stamp, 8), data.serial);
        assert_eq!(number(&stamp, 64), 8192);

        let mut edit = [0; 4192];
        edit[..4].copy_from_slice(b"LFE2");
        edit[4..6].copy_from_slice(&2u16.to_le_bytes());
        edit[8..16].copy_from_slice(&stamp[8..16]);
        edit[16..48].copy_from_slice(&stamp[16..48]);
        edit[48..64].copy_from_slice(&stamp[48..64]);
        edit[64..72].copy_from_slice(&4093u64.to_le_bytes());
        edit[80..84].copy_from_slice(&4096u32.to_le_bytes());
        for (i, byte) in edit[96..].iter_mut().enumerate() {
            *byte = (i % 239) as u8;
        }
        let result = unsafe { libc::ioctl(file.as_raw_fd(), EDIT as _, edit.as_mut_ptr()) };
        assert_eq!(result, 0, "EDIT: {}", std::io::Error::last_os_error());

        let after = state(&file);
        assert_eq!(number(&after, 56), number(&stamp, 56) + 1);
        assert_eq!(number(&after, 64), 12288);
        for fd in [&file, &alias] {
            let stat = fd.metadata().unwrap();
            assert_eq!(stat.len(), 12288);
            assert_eq!(
                stat.mtime(),
                i64::from_le_bytes(after[72..80].try_into().unwrap())
            );
            assert_eq!(
                stat.mtime_nsec(),
                u32::from_le_bytes(after[80..84].try_into().unwrap()) as i64
            );
            assert_eq!(read(fd, 4090, 16), {
                let mut expected: Vec<_> = (4090..4093).map(|i| (i % 251) as u8).collect();
                expected.extend((0..13).map(|i| (i % 239) as u8));
                expected
            });
            assert!(read(fd, 12288, 16).is_empty());
        }
        let status = f.workspace.status().unwrap();
        assert_eq!(
            status
                .projection_calls
                .iter()
                .find(|(name, _)| *name == "range_edit")
                .unwrap()
                .1,
            1
        );
        assert_eq!(status.range_accepted_payload_bytes, 4096);
        assert_eq!(status.range_shifted_suffix_bytes, 0);

        let report = f.workspace.commit(deadline()).unwrap();
        let root = match report.outcome {
            layerfs_bridge::contract::CommitOutcomeWire::Committed(committed) => committed.root,
            other => panic!("{other:?}"),
        };
        let committed = super::support::attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(committed.2, 12288);
        let mut expected: Vec<_> = (0..4093).map(|i| (i % 251) as u8).collect();
        expected.extend((0..4096).map(|i| (i % 239) as u8));
        expected.extend((4093..8192).map(|i| (i % 251) as u8));
        assert_eq!(f.native.bytes(committed.1, 0, expected.len()), expected);
        drop(file);
        drop(alias);
        mount.unmount(deadline()).unwrap();
        assert!(!fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .contains(&f.workspace.mount_path().display().to_string()));
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK mounted-projected-insert-Commit PASS");
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and native service"]
    fn kernel_range_variants() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        f.workspace.set_len(data.serial, 8192, deadline()).unwrap();
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.workspace.mount_path().join("data.bin"))
            .unwrap();
        let alias = File::open(f.workspace.mount_path().join("alias")).unwrap();
        let initial = read(&file, 0, 8192);
        let before = state(&file);

        let mut oversized = [0u8; 128];
        oversized[..4].copy_from_slice(b"LFB3");
        oversized[4..6].copy_from_slice(&3u16.to_le_bytes());
        oversized[80..88].copy_from_slice(&(8 * 1024 * 1024u64 + 1).to_le_bytes());
        assert_eq!(
            unsafe {
                libc::ioctl(
                    file.as_raw_fd(),
                    0xc080_f542u32 as _,
                    oversized.as_mut_ptr(),
                )
            },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ENOSPC)
        );
        assert_eq!(state(&file), before);
        assert_eq!(read(&file, 0, 8192), initial);

        let mut malformed = edit(&before, 4094, 0, b"ABCD");
        malformed[4191] = 1;
        assert_eq!(call_edit(&file, &mut malformed), Err(libc::EINVAL));
        assert_eq!(state(&file), before);
        assert_eq!(read(&file, 0, 8192), initial);

        let mut overwrite = edit(&before, 4094, 4, b"WXYZ");
        call_edit(&file, &mut overwrite).unwrap();
        let after_overwrite = state(&file);
        assert_eq!(number(&after_overwrite, 56), number(&before, 56) + 1);
        assert_eq!(number(&after_overwrite, 64), 8192);
        assert_eq!(
            read(&alias, 4092, 8),
            [
                initial[4092],
                initial[4093],
                b'W',
                b'X',
                b'Y',
                b'Z',
                initial[4098],
                initial[4099]
            ]
        );
        let backing = f.workspace.backing_status().unwrap();
        assert_eq!(
            call_edit(&file, &mut edit(&before, 4094, 4, b"WXYZ")),
            Err(libc::ESTALE)
        );
        let after_stale = f.workspace.backing_status().unwrap();
        assert_eq!(after_stale.payloads, backing.payloads);
        assert_eq!(after_stale.allocated_bytes, backing.allocated_bytes);
        assert_eq!(after_stale.reserved_bytes, backing.reserved_bytes);
        assert_eq!(state(&file), after_overwrite);

        let append = OpenOptions::new()
            .read(true)
            .append(true)
            .open(f.workspace.mount_path().join("data.bin"))
            .unwrap();
        assert_eq!(
            call_edit(&append, &mut edit(&after_overwrite, 4094, 4, b"ABCD")),
            Err(libc::EBADF)
        );
        assert_eq!(state(&file), after_overwrite);

        let mut delete = edit(&after_overwrite, 4094, 4, b"");
        call_edit(&file, &mut delete).unwrap();
        let after_delete = state(&file);
        assert_eq!(number(&after_delete, 56), number(&before, 56) + 2);
        assert_eq!(number(&after_delete, 64), 8188);
        let mut expected = initial;
        expected.drain(4094..4098);
        assert_eq!(read(&file, 0, 8188), expected);
        assert_eq!(alias.metadata().unwrap().len(), 8188);
        assert!(read(&alias, 8188, 1).is_empty());

        assert_eq!(file.write_at(b"OP", 5000).unwrap(), 2);
        expected[5000..5002].copy_from_slice(b"OP");
        assert_eq!(read(&alias, 4998, 6), expected[4998..5004]);
        let final_state = state(&file);
        assert!(number(&final_state, 56) > number(&after_delete, 56));
        let status = f.workspace.status().unwrap();
        assert_eq!(status.range_accepted_payload_bytes, 4);
        assert_eq!(status.range_shifted_suffix_bytes, 0);

        let report = f.workspace.commit(deadline()).unwrap();
        let root = match report.outcome {
            layerfs_bridge::contract::CommitOutcomeWire::Committed(committed) => committed.root,
            other => panic!("{other:?}"),
        };
        let committed = super::support::attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(committed.2, 8188);
        assert_eq!(f.native.bytes(committed.1, 0, expected.len()), expected);
        drop(append);
        drop(file);
        drop(alias);
        mount.unmount(deadline()).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK variants PASS");
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and native service"]
    fn kernel_range_read_only() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        let file = File::open(f.workspace.mount_path().join("data.bin")).unwrap();
        let before = state(&file);
        assert_eq!(
            call_edit(&file, &mut edit(&before, 0, 0, b"ABCD")),
            Err(libc::EROFS)
        );
        assert_eq!(state(&file), before);
        assert_eq!(
            f.workspace.status().unwrap().range_accepted_payload_bytes,
            0
        );
        drop(file);
        mount.unmount(deadline()).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK read-only PASS");
    }
}
