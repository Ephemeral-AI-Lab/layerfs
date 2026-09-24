//! Mounted Linux ioctl acknowledgement diagnostic; no Workspace or Store calls.
#[cfg(target_os = "linux")]
mod linux {
    use fuser::{
        Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
        IoctlFlags, MountOption, OpenFlags, ReplyAttr, ReplyData, ReplyEntry, ReplyIoctl,
        ReplyOpen, Request, Session,
    };
    use std::{
        ffi::OsStr,
        fs::{self, File, OpenOptions},
        io::{self, Read, Write},
        os::{
            fd::AsRawFd,
            unix::fs::{FileExt, MetadataExt},
        },
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Arc, Mutex},
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
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
    fn request() -> [u8; SIZE] {
        let mut input = [0; SIZE];
        input[..4].copy_from_slice(b"LFR1");
        input[4..6].copy_from_slice(&1u16.to_le_bytes());
        input[8..16].copy_from_slice(&4093u64.to_le_bytes());
        input[24..28].copy_from_slice(&4096u32.to_le_bytes());
        for (i, byte) in input[32..].iter_mut().enumerate() {
            *byte = (i % 239) as u8;
        }
        input
    }
    fn accepted_state() -> Vec<u8> {
        let mut bytes: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        bytes.splice(4093..4093, (0..4096).map(|i| (i % 239) as u8));
        bytes
    }

    struct State {
        size: u64,
        mtime: u64,
    }
    struct Probe {
        root: PathBuf,
        case: String,
        state: Arc<Mutex<State>>,
        notifier: Arc<Mutex<Option<fuser::Notifier>>>,
    }
    impl Probe {
        fn attr(&self, ino: u64) -> FileAttr {
            let state = self.state.lock().unwrap();
            let dir = ino == 1;
            let at = UNIX_EPOCH + Duration::from_secs(state.mtime);
            FileAttr {
                ino: INodeNo(ino),
                size: if dir { 0 } else { state.size },
                blocks: 0,
                atime: at,
                mtime: at,
                ctime: at,
                crtime: at,
                kind: if dir {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                },
                perm: if dir { 0o755 } else { 0o666 },
                nlink: if dir { 2 } else { 1 },
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                flags: 0,
            }
        }
    }
    impl Filesystem for Probe {
        fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
            if parent.0 == 1 && name == "file" {
                reply.entry(&TTL, &self.attr(2), Generation(1));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
            event(&self.root, &format!("getattr ino={}", ino.0));
            if ino.0 == 1 || ino.0 == 2 {
                reply.attr(&TTL, &self.attr(ino.0));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
            event(
                &self.root,
                &format!("open ino={} flags={:#x} fh=40 direct_io=1", ino.0, flags.0),
            );
            reply.opened(FileHandle(40), FopenFlags::FOPEN_DIRECT_IO);
        }
        fn read(
            &self,
            _req: &Request,
            ino: INodeNo,
            fh: FileHandle,
            offset: u64,
            size: u32,
            _flags: OpenFlags,
            _lock_owner: Option<fuser::LockOwner>,
            reply: ReplyData,
        ) {
            if ino.0 != 2 || fh.0 != 40 {
                reply.error(Errno::EBADF);
                return;
            }
            let len = self.state.lock().unwrap().size;
            let end = len.min(offset.saturating_add(size as u64));
            let accepted = accepted_state();
            let data = if offset < end {
                &accepted[offset as usize..end as usize]
            } else {
                &[]
            };
            event(
                &self.root,
                &format!(
                    "read offset={offset} requested={size} returned={} hex={}",
                    data.len(),
                    hex(data)
                ),
            );
            reply.data(data);
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
            event(
                &self.root,
                &format!(
                    "ioctl ino={} fh={} flags={flags:?} cmd={cmd:#x} in={} out={} hex={}",
                    ino.0,
                    fh.0,
                    input.len(),
                    out_size,
                    hex(input)
                ),
            );
            if ino.0 != 2 || fh.0 != 40 || cmd != CMD || input != request() {
                reply.error(Errno::EINVAL);
                return;
            }
            {
                let mut state = self.state.lock().unwrap();
                state.size = 12288;
                state.mtime += 1;
            }
            event(&self.root, "published size=12288 mtime=1700000001");
            if self.case == "ack_lost_reply" {
                fs::write(self.root.join("accepted-state.bin"), accepted_state()).unwrap();
                event(
                    &self.root,
                    "accepted artifact written; daemon exit 23 before reply",
                );
                std::process::exit(23);
            }
            if self.case == "ack_success" {
                let result =
                    self.notifier
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .inval_inode(INodeNo(2), 0, 0);
                event(&self.root, &format!("real inval_inode result={result:?}"));
                if result.is_err() {
                    reply.error(Errno::EIO);
                    return;
                }
            } else {
                event(&self.root, "test-only notification suppression");
            }
            reply.ioctl(0, &[]);
            event(
                &self.root,
                "reply method returned result=0 (delivery not acknowledged to daemon)",
            );
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
    fn read(file: &File, at: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut data = vec![0; len];
        let count = file.read_at(&mut data, at)?;
        data.truncate(count);
        Ok(data)
    }
    fn mounted(root: &Path) -> bool {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| line.contains(&root.join("mnt").display().to_string()))
    }
    fn daemon(case: &str, root: &Path) {
        let state = Arc::new(Mutex::new(State {
            size: 8192,
            mtime: 1_700_000_000,
        }));
        let notifier = Arc::new(Mutex::new(None));
        let fs = Probe {
            root: root.into(),
            case: case.into(),
            state,
            notifier: notifier.clone(),
        };
        let mut config = Config::default();
        config.mount_options = vec![
            MountOption::RW,
            MountOption::NoSuid,
            MountOption::NoDev,
            MountOption::DefaultPermissions,
            MountOption::NoAtime,
            MountOption::FSName("layerfs-241-ack".into()),
        ];
        let session = Session::new(fs, root.join("mnt"), &config).unwrap();
        let bg = session.spawn().unwrap();
        *notifier.lock().unwrap() = Some(bg.notifier());
        let mountinfo = fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .find(|line| line.contains(&root.join("mnt").display().to_string()))
            .unwrap()
            .to_string();
        fs::write(root.join("mountinfo"), mountinfo).unwrap();
        fs::write(root.join("ready"), "1").unwrap();
        let mut byte = [0];
        let _ = io::stdin().read(&mut byte);
        bg.umount_and_join().unwrap();
        event(root, "unmounted");
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
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("mnt/file"))
            .unwrap();
        let initial = file.metadata().unwrap();
        assert_eq!((initial.len(), initial.mtime()), (8192, 1_700_000_000));
        let result = ioctl(file.as_raw_fd());
        let syscall = format!(
            "ioctl={:?} errno={:?}",
            result.as_ref().map(|_| ()),
            result.as_ref().err().and_then(io::Error::raw_os_error)
        );
        let mut observations = vec![
            format!(
                "initial_size={} initial_mtime={}",
                initial.len(),
                initial.mtime()
            ),
            syscall,
        ];
        let decision = if result.is_ok() {
            let stat = file.metadata();
            let boundary = read(&file, 4090, 16);
            let eof = read(&file, 12288, 16);
            observations.push(format!(
                "fstat={:?}",
                stat.as_ref().map(|s| (s.len(), s.mtime()))
            ));
            observations.push(format!("boundary={:?}", boundary.as_ref().map(|b| hex(b))));
            observations.push(format!("eof={:?}", eof.as_ref().map(|b| hex(b))));
            let expected = &accepted_state()[4090..4106];
            if stat
                .as_ref()
                .is_ok_and(|s| s.len() == 12288 && s.mtime() > initial.mtime())
                && boundary.as_ref().is_ok_and(|b| b == expected)
                && eof.as_ref().is_ok_and(Vec::is_empty)
            {
                "SUCCESS"
            } else {
                "UNCERTAIN"
            }
        } else {
            "UNCERTAIN"
        };
        observations.push(format!("decision={decision} ioctl_calls=1 retry_calls=0"));
        fs::write(root.join("caller.log"), observations.join("\n") + "\n").unwrap();
        println!("{}", observations.join("\n"));
        if case == "ack_lost_reply" {
            assert_eq!(decision, "UNCERTAIN");
            assert_eq!(child.wait().unwrap().code(), Some(23));
            assert_eq!(
                fs::read(root.join("accepted-state.bin")).unwrap(),
                accepted_state()
            );
            let mount = std::ffi::CString::new(root.join("mnt").to_str().unwrap()).unwrap();
            assert_eq!(
                unsafe { libc::umount2(mount.as_ptr(), libc::MNT_DETACH) },
                0
            );
        } else {
            assert_eq!(
                decision,
                if case == "ack_success" {
                    "SUCCESS"
                } else {
                    "UNCERTAIN"
                }
            );
            assert!(result.is_ok());
            drop(file);
            drop(child.stdin.take());
            assert!(child.wait().unwrap().success());
        }
        assert!(!mounted(&root));
        println!("PROBE_CASE {case} PASS");
    }
}
