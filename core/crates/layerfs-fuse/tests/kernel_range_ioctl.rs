//! Actual mounted Linux STATE/EDIT route through the projected Workspace.
#[cfg(target_os = "linux")]
#[path = "../../layerfs-workspace/tests/support/native_workspace.rs"]
mod support;

#[cfg(target_os = "linux")]
mod linux {
    use super::support::{deadline, Fixture, Gate};
    use layerfs_workspace::{FileAccess, FileCreateOptions, FileOpenOptions, ReferenceScope};
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
    fn kernel_range_stage_lifecycle() {
        use sha2::{Digest, Sha256};
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        f.workspace.set_len(data.serial, 8192, deadline()).unwrap();
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().join("data.bin");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let other = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let before = state(&file);
        let original = read(&file, 0, 8192);
        let payload = vec![0x5a; 65536];
        let mut begin = [0u8; 128];
        begin[..4].copy_from_slice(b"LFB3");
        begin[4..6].copy_from_slice(&3u16.to_le_bytes());
        begin[8..64].copy_from_slice(&before[8..64]);
        begin[64..72].copy_from_slice(&4096u64.to_le_bytes());
        begin[80..88].copy_from_slice(&65536u64.to_le_bytes());
        begin[88..96].copy_from_slice(&65536u64.to_le_bytes());
        begin[96..128].copy_from_slice(&Sha256::digest(&payload));
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0xc080_f542u32 as _, begin.as_mut_ptr()) },
            0
        );
        assert_eq!(&begin[..8], b"LFB3\x03\0\0\0");
        let token: [u8; 16] = begin[8..24].try_into().unwrap();
        assert_ne!(token, [0; 16]);

        let mut fragment = [0u8; 4224];
        fragment[..4].copy_from_slice(b"LFD3");
        fragment[4..6].copy_from_slice(&3u16.to_le_bytes());
        fragment[6..8].copy_from_slice(&1u16.to_le_bytes());
        fragment[8..24].copy_from_slice(&token);
        fragment[32..40].copy_from_slice(&4096u64.to_le_bytes());
        fragment[40..44].copy_from_slice(&4096u32.to_le_bytes());
        fragment[128..].fill(0x5a);
        fragment[24..32].copy_from_slice(&4096u64.to_le_bytes());
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x5080_f543u32 as _, fragment.as_mut_ptr()) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EINVAL)
        );
        fragment[24..32].fill(0);
        assert_eq!(
            unsafe {
                libc::ioctl(
                    other.as_raw_fd(),
                    0x5080_f543u32 as _,
                    fragment.as_mut_ptr(),
                )
            },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EBADF)
        );
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x5080_f543u32 as _, fragment.as_mut_ptr()) },
            0
        );
        assert_eq!(state(&file), before);
        assert_eq!(read(&file, 0, 8192), original);

        let mut terminal = [0u8; 128];
        terminal[..4].copy_from_slice(b"LFX3");
        terminal[4..6].copy_from_slice(&3u16.to_le_bytes());
        terminal[8..24].copy_from_slice(&token);
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x4080_f545u32 as _, terminal.as_mut_ptr()) },
            0
        );
        terminal[..4].copy_from_slice(b"LFA3");
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x4080_f544u32 as _, terminal.as_mut_ptr()) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EBADF)
        );
        assert_eq!(state(&file), before);
        drop(file);
        drop(other);
        mount.unmount(deadline()).unwrap();
        f.workspace.commit(deadline()).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK staging-lifecycle PASS");
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and native service"]
    fn kernel_range_staged_apply() {
        use sha2::{Digest, Sha256};
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
        let original = read(&file, 0, 8192);
        let before = state(&file);
        let payload: Vec<u8> = (0..65536).map(|i| (i % 239) as u8).collect();
        let mut begin = [0u8; 128];
        begin[..4].copy_from_slice(b"LFB3");
        begin[4..6].copy_from_slice(&3u16.to_le_bytes());
        begin[8..64].copy_from_slice(&before[8..64]);
        begin[64..72].copy_from_slice(&4096u64.to_le_bytes());
        begin[80..88].copy_from_slice(&65536u64.to_le_bytes());
        begin[88..96].copy_from_slice(&65536u64.to_le_bytes());
        begin[96..128].copy_from_slice(&Sha256::digest(&payload));
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0xc080_f542u32 as _, begin.as_mut_ptr()) },
            0
        );
        let token: [u8; 16] = begin[8..24].try_into().unwrap();
        for (index, chunk) in payload.chunks(4096).enumerate() {
            let mut frame = [0u8; 4224];
            frame[..4].copy_from_slice(b"LFD3");
            frame[4..6].copy_from_slice(&3u16.to_le_bytes());
            frame[6..8].copy_from_slice(&1u16.to_le_bytes());
            frame[8..24].copy_from_slice(&token);
            frame[24..32].copy_from_slice(&((index * 4096) as u64).to_le_bytes());
            frame[32..40].copy_from_slice(&4096u64.to_le_bytes());
            frame[40..44].copy_from_slice(&4096u32.to_le_bytes());
            frame[128..].copy_from_slice(chunk);
            assert_eq!(
                unsafe { libc::ioctl(file.as_raw_fd(), 0x5080_f543u32 as _, frame.as_mut_ptr()) },
                0
            );
        }
        assert_eq!(state(&file), before);
        assert_eq!(read(&file, 0, 8192), original);
        let mut apply = [0u8; 128];
        apply[..4].copy_from_slice(b"LFA3");
        apply[4..6].copy_from_slice(&3u16.to_le_bytes());
        apply[8..24].copy_from_slice(&token);
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x4080_f544u32 as _, apply.as_mut_ptr()) },
            0
        );
        let after = state(&file);
        assert_eq!(number(&after, 56), number(&before, 56) + 1);
        assert_eq!(number(&after, 64), 8192 + 65536);
        assert_eq!(file.metadata().unwrap().len(), 8192 + 65536);
        assert_eq!(alias.metadata().unwrap().len(), 8192 + 65536);
        assert_eq!(read(&file, 4096, 65536), payload);
        assert!(read(&alias, 8192 + 65536, 1).is_empty());

        let mut zero_begin = [0u8; 128];
        zero_begin[..4].copy_from_slice(b"LFB3");
        zero_begin[4..6].copy_from_slice(&3u16.to_le_bytes());
        zero_begin[8..64].copy_from_slice(&after[8..64]);
        zero_begin[64..72].copy_from_slice(&(8192u64 + 65536).to_le_bytes());
        zero_begin[80..88].copy_from_slice(&4096u64.to_le_bytes());
        zero_begin[96..128].copy_from_slice(&Sha256::digest([0u8; 4096]));
        assert_eq!(
            unsafe {
                libc::ioctl(
                    file.as_raw_fd(),
                    0xc080_f542u32 as _,
                    zero_begin.as_mut_ptr(),
                )
            },
            0
        );
        let zero_token: [u8; 16] = zero_begin[8..24].try_into().unwrap();
        let mut zero_data = [0u8; 4224];
        zero_data[..4].copy_from_slice(b"LFD3");
        zero_data[4..6].copy_from_slice(&3u16.to_le_bytes());
        zero_data[6..8].copy_from_slice(&2u16.to_le_bytes());
        zero_data[8..24].copy_from_slice(&zero_token);
        zero_data[32..40].copy_from_slice(&4096u64.to_le_bytes());
        assert_eq!(
            unsafe {
                libc::ioctl(
                    file.as_raw_fd(),
                    0x5080_f543u32 as _,
                    zero_data.as_mut_ptr(),
                )
            },
            0
        );
        apply[8..24].copy_from_slice(&zero_token);
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x4080_f544u32 as _, apply.as_mut_ptr()) },
            0
        );
        let final_state = state(&file);
        assert_eq!(number(&final_state, 56), number(&before, 56) + 2);
        assert_eq!(number(&final_state, 64), 8192 + 65536 + 4096);
        assert_eq!(read(&alias, 8192 + 65536, 4096), vec![0; 4096]);
        assert!(read(&file, 8192 + 65536 + 4096, 1).is_empty());
        assert_eq!(
            f.workspace.status().unwrap().range_accepted_payload_bytes,
            65536
        );

        let report = f.workspace.commit(deadline()).unwrap();
        let root = match report.outcome {
            layerfs_bridge::contract::CommitOutcomeWire::Committed(committed) => committed.root,
            other => panic!("{other:?}"),
        };
        let committed = super::support::attr(f.native.attributes(root, b"data.bin"));
        let mut expected = original[..4096].to_vec();
        expected.extend(payload);
        expected.extend(&original[4096..]);
        expected.extend([0; 4096]);
        assert_eq!(committed.2, expected.len() as u64);
        assert_eq!(f.native.bytes(committed.1, 0, expected.len()), expected);
        drop(file);
        drop(alias);
        mount.unmount(deadline()).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK staged-apply PASS");
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and native service"]
    fn kernel_range_staged_bounds() {
        use sha2::{Digest, Sha256};
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().join("data.bin");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let other = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let before = state(&file);
        let original_len = number(&before, 64);
        let request = |stamp: &[u8; 88], logical: u64, literal: u64, digest: &[u8]| {
            let mut bytes = [0u8; 128];
            bytes[..4].copy_from_slice(b"LFB3");
            bytes[4..6].copy_from_slice(&3u16.to_le_bytes());
            bytes[8..64].copy_from_slice(&stamp[8..64]);
            bytes[64..72].copy_from_slice(&number(stamp, 64).to_le_bytes());
            bytes[80..88].copy_from_slice(&logical.to_le_bytes());
            bytes[88..96].copy_from_slice(&literal.to_le_bytes());
            bytes[96..128].copy_from_slice(digest);
            bytes
        };
        let call = |file: &File, cmd: u32, bytes: &mut [u8]| -> Result<(), i32> {
            if unsafe { libc::ioctl(file.as_raw_fd(), cmd as _, bytes.as_mut_ptr()) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error().raw_os_error().unwrap())
            }
        };
        let terminal = |magic: &[u8; 4], token: &[u8; 16]| {
            let mut bytes = [0u8; 128];
            bytes[..4].copy_from_slice(magic);
            bytes[4..6].copy_from_slice(&3u16.to_le_bytes());
            bytes[8..24].copy_from_slice(token);
            bytes
        };
        let mut oversized = request(&before, 8 * 1024 * 1024 + 1, 0, &[0; 32]);
        assert_eq!(call(&file, 0xc080_f542, &mut oversized), Err(libc::ENOSPC));
        let mut stale = request(&before, 1, 1, &Sha256::digest(b"X"));
        stale[56..64].copy_from_slice(&(number(&before, 56) + 1).to_le_bytes());
        assert_eq!(call(&file, 0xc080_f542, &mut stale), Err(libc::ESTALE));
        assert_eq!(state(&file), before);

        let mut bad = request(&before, 4096, 4096, &[0; 32]);
        call(&file, 0xc080_f542, &mut bad).unwrap();
        let bad_token: [u8; 16] = bad[8..24].try_into().unwrap();
        let mut bytes = [0u8; 4224];
        bytes[..4].copy_from_slice(b"LFD3");
        bytes[4..6].copy_from_slice(&3u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
        bytes[8..24].copy_from_slice(&bad_token);
        bytes[32..40].copy_from_slice(&4096u64.to_le_bytes());
        bytes[40..44].copy_from_slice(&4096u32.to_le_bytes());
        bytes[128..].fill(0x5a);
        call(&file, 0x5080_f543, &mut bytes).unwrap();
        assert_eq!(state(&file), before);
        assert_eq!(
            call(&file, 0x4080_f544, &mut terminal(b"LFA3", &bad_token)),
            Err(libc::EINVAL)
        );
        assert_eq!(state(&file), before);

        let mut zero_hash = Sha256::new();
        for _ in 0..2048 {
            zero_hash.update([0u8; 4096]);
        }
        let mut full = request(&before, 8 * 1024 * 1024, 0, &zero_hash.finalize());
        call(&file, 0xc080_f542, &mut full).unwrap();
        let token: [u8; 16] = full[8..24].try_into().unwrap();
        let mut concurrent = request(&before, 1, 1, &Sha256::digest(b"X"));
        assert_eq!(
            call(&other, 0xc080_f542, &mut concurrent),
            Err(libc::ENOSPC)
        );
        let mut zero_data = [0u8; 4224];
        zero_data[..4].copy_from_slice(b"LFD3");
        zero_data[4..6].copy_from_slice(&3u16.to_le_bytes());
        zero_data[6..8].copy_from_slice(&2u16.to_le_bytes());
        zero_data[8..24].copy_from_slice(&token);
        zero_data[32..40].copy_from_slice(&(8 * 1024 * 1024u64).to_le_bytes());
        assert_eq!(call(&other, 0x5080_f543, &mut zero_data), Err(libc::EBADF));
        zero_data[24..32].copy_from_slice(&1u64.to_le_bytes());
        assert_eq!(call(&file, 0x5080_f543, &mut zero_data), Err(libc::EINVAL));
        zero_data[24..32].fill(0);
        call(&file, 0x5080_f543, &mut zero_data).unwrap();
        assert_eq!(state(&file), before);
        call(&file, 0x4080_f544, &mut terminal(b"LFA3", &token)).unwrap();
        let after = state(&file);
        assert_eq!(number(&after, 56), number(&before, 56) + 1);
        assert_eq!(number(&after, 64), original_len + 8 * 1024 * 1024);
        assert_eq!(
            file.metadata().unwrap().len(),
            original_len + 8 * 1024 * 1024
        );
        assert_eq!(
            other.metadata().unwrap().len(),
            original_len + 8 * 1024 * 1024
        );
        assert_eq!(read(&file, original_len, 4096), vec![0; 4096]);
        assert!(read(&file, original_len + 8 * 1024 * 1024, 1).is_empty());
        assert_eq!(
            f.workspace.status().unwrap().range_accepted_payload_bytes,
            0
        );
        let report = f.workspace.commit(deadline()).unwrap();
        let root = match report.outcome {
            layerfs_bridge::contract::CommitOutcomeWire::Committed(committed) => committed.root,
            other => panic!("{other:?}"),
        };
        let committed = super::support::attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(committed.2, original_len + 8 * 1024 * 1024);
        assert_eq!(
            f.native.bytes(committed.1, original_len, 4096),
            vec![0; 4096]
        );
        drop(file);
        drop(other);
        mount.unmount(deadline()).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK staged-bounds PASS");
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and one 30-second expiry"]
    fn kernel_range_stage_cleanup() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().join("data.bin");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let other = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let before = state(&file);
        let begin = |fd: &File| {
            let mut frame = [0u8; 128];
            frame[..4].copy_from_slice(b"LFB3");
            frame[4..6].copy_from_slice(&3u16.to_le_bytes());
            frame[8..64].copy_from_slice(&before[8..64]);
            frame[64..72].copy_from_slice(&number(&before, 64).to_le_bytes());
            frame[80..88].copy_from_slice(&(8 * 1024 * 1024u64).to_le_bytes());
            assert_eq!(
                unsafe { libc::ioctl(fd.as_raw_fd(), 0xc080_f542u32 as _, frame.as_mut_ptr()) },
                0
            );
            assert_ne!(&frame[8..24], &[0; 16]);
        };
        begin(&file);
        let handles = f.workspace.status().unwrap().projection_handles;
        drop(file);
        for _ in 0..100 {
            if f.workspace.status().unwrap().projection_handles + 1 == handles {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            f.workspace.status().unwrap().projection_handles + 1,
            handles
        );
        begin(&other); // Owner close released its full 8 MiB reservation.
        std::thread::sleep(std::time::Duration::from_secs(31));
        let third = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        begin(&third); // Idle deadline sweeper released the second reservation.
        assert_eq!(state(&third), before);
        let closer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            drop(other);
            drop(third);
        });
        mount.unmount(deadline()).unwrap(); // Begins with a private stage outstanding.
        closer.join().unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK stage-cleanup PASS");
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and sparse 4 GiB logical file"]
    fn kernel_range_staged_max_file_math() {
        use sha2::{Digest, Sha256};
        const MAX: u64 = 4 * 1024 * 1024 * 1024;
        let f = Fixture::new(Gate::None);
        let (created, local_handle) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"limit.bin",
                FileCreateOptions {
                    mode: 0o600,
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
        f.workspace
            .set_len(created.serial, MAX, deadline())
            .unwrap();
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.workspace.mount_path().join("limit.bin"))
            .unwrap();
        let before = state(&file);
        assert_eq!(number(&before, 64), MAX);
        let request = |offset: u64, deleted: u64, logical: u64| {
            let mut bytes = [0u8; 128];
            bytes[..4].copy_from_slice(b"LFB3");
            bytes[4..6].copy_from_slice(&3u16.to_le_bytes());
            bytes[8..64].copy_from_slice(&before[8..64]);
            bytes[64..72].copy_from_slice(&offset.to_le_bytes());
            bytes[72..80].copy_from_slice(&deleted.to_le_bytes());
            bytes[80..88].copy_from_slice(&logical.to_le_bytes());
            bytes[88..96].copy_from_slice(&1u64.to_le_bytes());
            bytes[96..128].copy_from_slice(&Sha256::digest(b"Z"));
            bytes
        };
        let call = |cmd: u32, bytes: &mut [u8]| -> Result<(), i32> {
            if unsafe { libc::ioctl(file.as_raw_fd(), cmd as _, bytes.as_mut_ptr()) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error().raw_os_error().unwrap())
            }
        };
        let mut overflow = request(u64::MAX, 1, 1);
        assert_eq!(call(0xc080_f542, &mut overflow), Err(libc::EINVAL));
        let mut too_long = request(MAX, 0, 1);
        assert_eq!(call(0xc080_f542, &mut too_long), Err(libc::ENOSPC));
        let mut exact = request(MAX - 1, 1, 1);
        call(0xc080_f542, &mut exact).unwrap();
        let mut abort = [0u8; 128];
        abort[..4].copy_from_slice(b"LFX3");
        abort[4..6].copy_from_slice(&3u16.to_le_bytes());
        abort[8..24].copy_from_slice(&exact[8..24]);
        call(0x4080_f545, &mut abort).unwrap();
        assert_eq!(state(&file), before);
        drop(file);
        mount.unmount(deadline()).unwrap();
        f.workspace.release(local_handle).unwrap();
        f.workspace
            .unlink(f.workspace.root().serial, b"limit.bin", deadline())
            .unwrap();
        f.workspace
            .forget(created.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.commit(deadline()).unwrap(); // The 4 GiB temporary is gone; no data copy.
        f.workspace.close_clean().unwrap();
        println!("KERNEL_RANGE_CHECK staged-max-file-math PASS");
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
