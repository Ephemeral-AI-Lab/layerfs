//! Mounted Linux FUSE failure boundaries for the provisional #241 ioctl carrier.
#[cfg(target_os = "linux")]
mod linux {
    use fuser::{
        Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
        IoctlFlags, MountOption, OpenFlags, ReplyAttr, ReplyEmpty, ReplyEntry, ReplyIoctl,
        ReplyOpen, Request, Session,
    };
    use std::{
        ffi::OsStr,
        fs::{self, File, OpenOptions},
        io::{self, Read, Write},
        os::{fd::AsRawFd, unix::fs::MetadataExt},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        thread,
        time::{Duration, UNIX_EPOCH},
    };

    const SIZE: usize = 4128;
    const CMD: u32 = (1 << 30) | ((SIZE as u32) << 16) | (0xf5 << 8) | 0x41;
    const TTL: Duration = Duration::from_secs(60);

    fn event(root: &Path, line: &str) {
        writeln!(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join("events.log"))
                .unwrap(),
            "{line}"
        )
        .unwrap();
    }

    fn request() -> [u8; SIZE] {
        let mut input = [0; SIZE];
        input[..4].copy_from_slice(b"LFR1");
        input[4..6].copy_from_slice(&1u16.to_le_bytes());
        input[8..16].copy_from_slice(&4093u64.to_le_bytes());
        input[24..28].copy_from_slice(&4096u32.to_le_bytes());
        for (index, byte) in input[32..].iter_mut().enumerate() {
            *byte = (index % 239) as u8;
        }
        input
    }

    fn accepted_state() -> Vec<u8> {
        let mut bytes: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        bytes.splice(4093..4093, (0..4096).map(|i| (i % 239) as u8));
        bytes
    }

    fn attr(ino: u64, size: u64, mtime: u64) -> FileAttr {
        let directory = ino == 1;
        let at = UNIX_EPOCH + Duration::from_secs(mtime);
        FileAttr {
            ino: INodeNo(ino),
            size: if directory { 0 } else { size },
            blocks: 0,
            atime: at,
            mtime: at,
            ctime: at,
            crtime: at,
            kind: if directory {
                FileType::Directory
            } else {
                FileType::RegularFile
            },
            perm: if directory { 0o755 } else { 0o666 },
            nlink: if directory { 2 } else { 1 },
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        }
    }

    struct Probe {
        root: PathBuf,
        case: String,
    }

    impl Filesystem for Probe {
        fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
            if parent.0 == 1 && name == "file" {
                reply.entry(&TTL, &attr(2, 8192, 1_700_000_000), Generation(1));
            } else {
                reply.error(Errno::ENOENT);
            }
        }

        fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
            if ino.0 == 1 || ino.0 == 2 {
                reply.attr(&TTL, &attr(ino.0, 8192, 1_700_000_000));
            } else {
                reply.error(Errno::ENOENT);
            }
        }

        fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
            event(
                &self.root,
                &format!("open ino={} flags={:#x} fh=40", ino.0, flags.0),
            );
            reply.opened(FileHandle(40), FopenFlags::FOPEN_DIRECT_IO);
        }

        fn release(
            &self,
            _req: &Request,
            ino: INodeNo,
            fh: FileHandle,
            _flags: OpenFlags,
            _lock_owner: Option<fuser::LockOwner>,
            _flush: bool,
            reply: ReplyEmpty,
        ) {
            event(&self.root, &format!("release ino={} fh={}", ino.0, fh.0));
            reply.ok();
        }

        fn ioctl(
            &self,
            _req: &Request,
            ino: INodeNo,
            fh: FileHandle,
            flags: IoctlFlags,
            cmd: u32,
            input: &[u8],
            out_size: u32,
            reply: ReplyIoctl,
        ) {
            let hex: String = input.iter().map(|b| format!("{b:02x}")).collect();
            event(
                &self.root,
                &format!(
                    "ioctl ino={} fh={} flags={flags:?} cmd={cmd:#x} in={} out={} hex={hex}",
                    ino.0,
                    fh.0,
                    input.len(),
                    out_size
                ),
            );
            if cmd != CMD || ino.0 != 2 || fh.0 != 40 || input != request() {
                event(&self.root, "refused malformed request");
                reply.error(Errno::EINVAL);
            } else if self.case == "ro_mount" {
                event(
                    &self.root,
                    "refused readonly mount before publication errno=EROFS",
                );
                reply.error(Errno::EROFS);
            } else if self.case == "lost_reply" || self.case == "lost_reply_state" {
                if self.case == "lost_reply_state" {
                    fs::write(self.root.join("accepted-state.bin"), accepted_state()).unwrap();
                }
                event(
                    &self.root,
                    "published size=12288 mtime=1700000001; process exit before reply",
                );
                std::process::exit(23);
            } else {
                event(&self.root, "unexpected ioctl callback");
                reply.error(Errno::EIO);
            }
        }
    }

    fn ioctl(fd: i32) -> io::Result<()> {
        let mut input = request();
        let rc = unsafe { libc::ioctl(fd, CMD as _, input.as_mut_ptr()) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn mounted(root: &Path) -> bool {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| line.contains(&root.join("mnt").display().to_string()))
    }

    fn daemon(case: &str, root: &Path) {
        let mut config = Config::default();
        config.mount_options = vec![
            if case == "ro_mount" {
                MountOption::RO
            } else {
                MountOption::RW
            },
            MountOption::NoSuid,
            MountOption::NoDev,
            MountOption::DefaultPermissions,
            MountOption::NoAtime,
            MountOption::FSName("layerfs-241-failure".into()),
        ];
        let session = Session::new(
            Probe {
                root: root.into(),
                case: case.into(),
            },
            root.join("mnt"),
            &config,
        )
        .unwrap();
        let bg = session.spawn().unwrap();
        let notifier = bg.notifier();
        let mountinfo = fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .find(|line| line.contains(&root.join("mnt").display().to_string()))
            .unwrap()
            .to_string();
        fs::write(root.join("mountinfo"), &mountinfo).unwrap();
        event(root, &format!("mount {mountinfo}"));
        fs::write(root.join("ready"), "1").unwrap();
        let mut byte = [0];
        let _ = io::stdin().read(&mut byte);
        bg.umount_and_join().unwrap();
        event(root, "unmounted");
        if case == "notifier_after_unmount" {
            let result = notifier.inval_inode(INodeNo(2), 0, 0);
            event(root, &format!("real notifier after unmount: {result:?}"));
            assert!(
                result.is_err(),
                "notifier unexpectedly succeeded after unmount"
            );
        }
    }

    fn wait_for(root: &Path, needle: &str) {
        for _ in 0..200 {
            if fs::read_to_string(root.join("events.log"))
                .unwrap_or_default()
                .contains(needle)
            {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {needle}");
    }

    #[test]
    #[ignore = "requires privileged Linux Docker FUSE"]
    fn probe() {
        let case = std::env::var("PROBE_CASE").unwrap();
        let root = PathBuf::from(std::env::var("PROBE_ROOT").unwrap());
        if std::env::var_os("PROBE_DAEMON").is_some() {
            daemon(&case, &root);
            return;
        }
        fs::create_dir_all(root.join("mnt")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "linux::probe", "--nocapture"])
            .env("PROBE_DAEMON", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        for _ in 0..500 {
            if root.join("ready").exists() {
                break;
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "daemon exited before ready"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(root.join("ready").exists());
        let path = root.join("mnt/file");
        match case.as_str() {
            "ro_mount" => {
                let mountinfo = fs::read_to_string(root.join("mountinfo")).unwrap();
                assert!(
                    mountinfo.contains(" - fuse ") && mountinfo.contains(" ro,"),
                    "{mountinfo}"
                );
                let file = File::open(&path).unwrap();
                let initial = file.metadata().unwrap();
                let err = ioctl(file.as_raw_fd()).unwrap_err();
                println!("caller ioctl errno={:?}", err.raw_os_error());
                assert_eq!(err.raw_os_error(), Some(libc::EROFS));
                let after = file.metadata().unwrap();
                assert_eq!(
                    (after.len(), after.mtime()),
                    (initial.len(), initial.mtime())
                );
            }
            "closed_fd" => {
                let file = File::open(&path).unwrap();
                let fd = file.as_raw_fd();
                drop(file);
                wait_for(&root, "release ino=2 fh=40");
                let err = ioctl(fd).unwrap_err();
                println!("caller ioctl closed fd={fd} errno={:?}", err.raw_os_error());
                assert_eq!(err.raw_os_error(), Some(libc::EBADF));
            }
            "lost_reply" | "lost_reply_state" => {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .unwrap();
                let err = ioctl(file.as_raw_fd()).unwrap_err();
                println!(
                    "caller ioctl after daemon exit errno={:?}",
                    err.raw_os_error()
                );
                assert!([libc::EIO, libc::ENOTCONN, libc::ECONNABORTED]
                    .contains(&err.raw_os_error().unwrap_or_default()));
                wait_for(&root, "published size=12288");
                if case == "lost_reply_state" {
                    assert_eq!(
                        fs::read(root.join("accepted-state.bin")).unwrap(),
                        accepted_state()
                    );
                }
                let status = child.wait().unwrap();
                assert_eq!(status.code(), Some(23));
                let mount = std::ffi::CString::new(root.join("mnt").to_str().unwrap()).unwrap();
                let rc = unsafe { libc::umount2(mount.as_ptr(), libc::MNT_DETACH) };
                println!(
                    "cleanup umount2 rc={rc} errno={:?}",
                    io::Error::last_os_error().raw_os_error()
                );
                assert!(!mounted(&root), "mount survived daemon death cleanup");
                println!("PROBE_CASE {case} PASS");
                return;
            }
            "notifier_after_unmount" => {}
            _ => panic!("unknown case {case}"),
        }
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        assert!(!mounted(&root));
        let events = fs::read_to_string(root.join("events.log")).unwrap();
        if case == "ro_mount" {
            assert!(!events.contains("published"));
        } else if case == "closed_fd" {
            assert!(!events.contains("ioctl ino="));
        } else {
            assert!(events.contains("real notifier after unmount: Err("));
        }
        println!("PROBE_CASE {case} PASS");
    }
}
