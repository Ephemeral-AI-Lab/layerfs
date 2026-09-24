//! Exact-size, test-only #241 STATE/EDIT Linux FUSE protocol probe.
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

    const STATE: u32 = 0xc058_f540;
    const EDIT: u32 = 0x5060_f541;
    const TTL: Duration = Duration::from_secs(60);
    const INCARNATION: [u8; 32] = [0x24; 32];

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Stamp {
        incarnation: [u8; 32],
        generation: u64,
        revision: u64,
    }
    const INITIAL: Stamp = Stamp {
        incarnation: INCARNATION,
        generation: 7,
        revision: 3,
    };
    const EDITED: Stamp = Stamp {
        incarnation: INCARNATION,
        generation: 7,
        revision: 4,
    };
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct StateReply {
        inode: u64,
        stamp: Stamp,
        length: u64,
        mtime_sec: i64,
        mtime_nsec: u32,
    }
    const BEFORE: StateReply = StateReply {
        inode: 2,
        stamp: INITIAL,
        length: 8192,
        mtime_sec: 1_700_000_000,
        mtime_nsec: 0,
    };
    const AFTER: StateReply = StateReply {
        inode: 2,
        stamp: EDITED,
        length: 12288,
        mtime_sec: 1_700_000_001,
        mtime_nsec: 0,
    };

    fn log(root: &Path, file: &str, line: &str) {
        writeln!(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join(file))
                .unwrap(),
            "{line}"
        )
        .unwrap();
    }
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }
    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }
    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }
    fn i64_at(bytes: &[u8], at: usize) -> i64 {
        i64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }
    fn state_request() -> Vec<u8> {
        let mut bytes = vec![0; 88];
        bytes[..4].copy_from_slice(b"LFS2");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
        bytes
    }
    fn edit_request(revision: u64) -> Vec<u8> {
        let mut bytes = vec![0; 4192];
        bytes[..4].copy_from_slice(b"LFE2");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[8..16].copy_from_slice(&2u64.to_le_bytes());
        bytes[16..48].copy_from_slice(&INCARNATION);
        bytes[48..56].copy_from_slice(&7u64.to_le_bytes());
        bytes[56..64].copy_from_slice(&revision.to_le_bytes());
        bytes[64..72].copy_from_slice(&4093u64.to_le_bytes());
        bytes[80..84].copy_from_slice(&4096u32.to_le_bytes());
        for (i, byte) in bytes[96..].iter_mut().enumerate() {
            *byte = (i % 239) as u8;
        }
        bytes
    }
    fn encode_state(response: StateReply) -> [u8; 88] {
        let mut bytes = [0; 88];
        bytes[..4].copy_from_slice(b"LFS2");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[8..16].copy_from_slice(&response.inode.to_le_bytes());
        bytes[16..48].copy_from_slice(&response.stamp.incarnation);
        bytes[48..56].copy_from_slice(&response.stamp.generation.to_le_bytes());
        bytes[56..64].copy_from_slice(&response.stamp.revision.to_le_bytes());
        bytes[64..72].copy_from_slice(&response.length.to_le_bytes());
        bytes[72..80].copy_from_slice(&response.mtime_sec.to_le_bytes());
        bytes[80..84].copy_from_slice(&response.mtime_nsec.to_le_bytes());
        bytes
    }
    fn decode_state(bytes: &[u8]) -> StateReply {
        assert_eq!(bytes.len(), 88);
        assert_eq!(&bytes[..4], b"LFS2");
        assert_eq!(u16_at(bytes, 4), 2);
        assert!(bytes[6..8] == [0; 2] && bytes[84..88] == [0; 4]);
        StateReply {
            inode: u64_at(bytes, 8),
            stamp: Stamp {
                incarnation: bytes[16..48].try_into().unwrap(),
                generation: u64_at(bytes, 48),
                revision: u64_at(bytes, 56),
            },
            length: u64_at(bytes, 64),
            mtime_sec: i64_at(bytes, 72),
            mtime_nsec: u32_at(bytes, 80),
        }
    }
    fn accepted_bytes() -> Vec<u8> {
        let mut bytes: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        bytes.splice(4093..4093, (0..4096).map(|i| (i % 239) as u8));
        bytes
    }

    struct StateData {
        current: StateReply,
        publications: u64,
    }
    struct Probe {
        root: PathBuf,
        case: String,
        state: Arc<Mutex<StateData>>,
        notifier: Arc<Mutex<Option<fuser::Notifier>>>,
    }
    impl Probe {
        fn attr(&self, ino: u64) -> FileAttr {
            let current = self.state.lock().unwrap().current;
            let dir = ino == 1;
            let at = UNIX_EPOCH + Duration::from_secs(current.mtime_sec as u64);
            FileAttr {
                ino: INodeNo(ino),
                size: if dir { 0 } else { current.length },
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
        fn refuse(&self, reply: ReplyIoctl, errno: Errno, reason: &str) {
            log(
                &self.root,
                "events.log",
                &format!("reply errno={errno:?} reason={reason}"),
            );
            reply.error(errno);
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
            log(&self.root, "events.log", &format!("getattr ino={}", ino.0));
            if ino.0 == 1 || ino.0 == 2 {
                reply.attr(&TTL, &self.attr(ino.0));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
            log(
                &self.root,
                "events.log",
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
            let len = self.state.lock().unwrap().current.length;
            let end = len.min(offset.saturating_add(size as u64));
            let bytes = if len == 8192 {
                (0..8192).map(|i| (i % 251) as u8).collect::<Vec<_>>()
            } else {
                accepted_bytes()
            };
            let data = if offset < end {
                &bytes[offset as usize..end as usize]
            } else {
                &[]
            };
            log(
                &self.root,
                "events.log",
                &format!(
                    "read at={offset} want={size} got={} hex={}",
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
            log(
                &self.root,
                "events.log",
                &format!(
                    "ioctl ino={} fh={} flags={flags:?} cmd={cmd:#x} in={} out={} hex={}",
                    ino.0,
                    fh.0,
                    input.len(),
                    out_size,
                    hex(input)
                ),
            );
            if ino.0 != 2 || fh.0 != 40 {
                self.refuse(reply, Errno::EBADF, "handle");
                return;
            }
            if cmd == STATE {
                if input != state_request() || out_size != 88 {
                    self.refuse(reply, Errno::EINVAL, "state-layout");
                    return;
                }
                let current = self.state.lock().unwrap().current;
                let bytes = encode_state(current);
                log(
                    &self.root,
                    "events.log",
                    &format!("reply state bytes=88 hex={}", hex(&bytes)),
                );
                reply.ioctl(0, &bytes);
                log(&self.root, "events.log", "reply state method returned");
                return;
            }
            if cmd != EDIT {
                self.refuse(reply, Errno::ENOTTY, "command");
                return;
            }
            if input.len() != 4192
                || out_size != 0
                || input[..4] != *b"LFE2"
                || u16_at(input, 4) != 2
                || input[6..8] != [0; 2]
                || u64_at(input, 8) != 2
                || input[16..48] != INCARNATION
                || u64_at(input, 48) != 7
                || u64_at(input, 64) != 4093
                || u64_at(input, 72) != 0
                || u32_at(input, 80) != 4096
                || input[84..96].iter().any(|&b| b != 0)
                || input[96..]
                    .iter()
                    .enumerate()
                    .any(|(i, &b)| b != (i % 239) as u8)
            {
                self.refuse(reply, Errno::EINVAL, "edit-layout");
                return;
            }
            let mut s = self.state.lock().unwrap();
            if u64_at(input, 56) != s.current.stamp.revision {
                self.refuse(reply, Errno::ESTALE, "revision");
                return;
            }
            if s.publications != 0 {
                self.refuse(reply, Errno::EALREADY, "fixture-single-edit");
                return;
            }
            s.current = AFTER;
            s.publications += 1;
            log(
                &self.root,
                "events.log",
                &format!(
                    "published revision=4 length=12288 publications={}",
                    s.publications
                ),
            );
            drop(s);
            if self.case == "lost_reply_unknown" {
                fs::write(self.root.join("accepted-state.bin"), accepted_bytes()).unwrap();
                log(
                    &self.root,
                    "events.log",
                    "accepted artifact written; daemon exit 23 before reply",
                );
                std::process::exit(23);
            }
            if self.case == "suppressed_unknown" {
                log(
                    &self.root,
                    "events.log",
                    "test-only notification suppression",
                );
            } else {
                let result =
                    self.notifier
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .inval_inode(INodeNo(2), 0, 0);
                log(
                    &self.root,
                    "events.log",
                    &format!("real inval_inode result={result:?}"),
                );
                if result.is_err() {
                    self.refuse(reply, Errno::EIO, "notify-after-publication");
                    return;
                }
            }
            log(&self.root, "events.log", "reply edit success bytes=0");
            reply.ioctl(0, &[]);
            log(&self.root, "events.log", "reply edit method returned");
        }
    }

    fn call(
        root: &Path,
        file: &File,
        name: &str,
        cmd: u32,
        mut bytes: Vec<u8>,
    ) -> io::Result<Vec<u8>> {
        let before = bytes.clone();
        log(
            root,
            "caller.log",
            &format!(
                "call={name} cmd={cmd:#x} input_len={} input_hex={}",
                bytes.len(),
                hex(&bytes)
            ),
        );
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), cmd as _, bytes.as_mut_ptr()) };
        if rc < 0 {
            let error = io::Error::last_os_error();
            log(
                root,
                "caller.log",
                &format!(
                    "return={name} error={:?} errno={:?}",
                    error,
                    error.raw_os_error()
                ),
            );
            Err(error)
        } else {
            log(
                root,
                "caller.log",
                &format!(
                    "return={name} rc={rc} output_len={} output_hex={} input_unchanged={}",
                    if cmd == STATE { 88 } else { 0 },
                    if cmd == STATE {
                        hex(&bytes)
                    } else {
                        String::new()
                    },
                    cmd != EDIT || bytes == before
                ),
            );
            if cmd == EDIT {
                assert_eq!(bytes, before);
            }
            Ok(bytes)
        }
    }
    fn read(file: &File, at: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0; len];
        let n = file.read_at(&mut out, at).unwrap();
        out.truncate(n);
        out
    }
    fn readback(root: &Path, file: &File) -> bool {
        let stat = file.metadata().unwrap();
        let boundary = read(file, 4090, 16);
        let eof = read(file, 12288, 16);
        log(
            root,
            "caller.log",
            &format!(
                "readback size={} mtime={} boundary={} eof_len={}",
                stat.len(),
                stat.mtime(),
                hex(&boundary),
                eof.len()
            ),
        );
        stat.len() == 12288
            && stat.mtime() == 1_700_000_001
            && boundary == accepted_bytes()[4090..4106]
            && eof.is_empty()
    }
    fn mounted(root: &Path) -> bool {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| line.contains(&root.join("mnt").display().to_string()))
    }
    fn daemon(case: &str, root: &Path) {
        let state = Arc::new(Mutex::new(StateData {
            current: BEFORE,
            publications: 0,
        }));
        let notifier = Arc::new(Mutex::new(None));
        let fs = Probe {
            root: root.into(),
            case: case.into(),
            state: state.clone(),
            notifier: notifier.clone(),
        };
        let mut config = Config::default();
        config.mount_options = vec![
            MountOption::RW,
            MountOption::NoSuid,
            MountOption::NoDev,
            MountOption::DefaultPermissions,
            MountOption::NoAtime,
            MountOption::FSName("layerfs-241-final-protocol".into()),
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
        let s = state.lock().unwrap();
        log(
            root,
            "events.log",
            &format!(
                "final publications={} revision={} size={}",
                s.publications, s.current.stamp.revision, s.current.length
            ),
        );
        log(root, "events.log", "unmounted");
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
        assert_eq!(
            (
                file.metadata().unwrap().len(),
                file.metadata().unwrap().mtime()
            ),
            (8192, 1_700_000_000)
        );
        assert_eq!(
            decode_state(&call(&root, &file, "state-initial", STATE, state_request()).unwrap()),
            BEFORE
        );
        match case.as_str() {
            "state_exact" => {
                log(
                    &root,
                    "caller.log",
                    "decision=STATE_ONLY edit_calls=0 retry_calls=0",
                );
            }
            "edit_state" => {
                call(&root, &file, "edit", EDIT, edit_request(3)).unwrap();
                assert!(readback(&root, &file));
                assert_eq!(
                    decode_state(
                        &call(&root, &file, "state-after", STATE, state_request()).unwrap()
                    ),
                    AFTER
                );
                log(
                    &root,
                    "caller.log",
                    "decision=SUCCESS edit_calls=1 retry_calls=0",
                );
            }
            "stale_refusal" => {
                assert_eq!(
                    call(&root, &file, "edit-stale", EDIT, edit_request(2))
                        .unwrap_err()
                        .raw_os_error(),
                    Some(libc::ESTALE)
                );
                assert_eq!(
                    decode_state(
                        &call(&root, &file, "state-after", STATE, state_request()).unwrap()
                    ),
                    BEFORE
                );
                assert_eq!(
                    (
                        file.metadata().unwrap().len(),
                        file.metadata().unwrap().mtime()
                    ),
                    (8192, 1_700_000_000)
                );
                assert_eq!(
                    read(&file, 4090, 16),
                    (4090..4106).map(|i| (i % 251) as u8).collect::<Vec<_>>()
                );
                log(
                    &root,
                    "caller.log",
                    "decision=DEFINITE_REFUSAL edit_calls=1 retry_calls=0",
                );
            }
            "suppressed_unknown" => {
                call(&root, &file, "edit", EDIT, edit_request(3)).unwrap();
                assert!(!readback(&root, &file));
                assert_eq!(
                    decode_state(
                        &call(&root, &file, "state-after", STATE, state_request()).unwrap()
                    ),
                    AFTER
                );
                log(
                    &root,
                    "caller.log",
                    "decision=UNKNOWN edit_calls=1 retry_calls=0",
                );
            }
            "lost_reply_unknown" => {
                let error = call(&root, &file, "edit", EDIT, edit_request(3)).unwrap_err();
                assert!([libc::ECONNABORTED, libc::ENOTCONN, libc::EIO]
                    .contains(&error.raw_os_error().unwrap_or_default()));
                assert_eq!(child.wait().unwrap().code(), Some(23));
                assert_eq!(
                    fs::read(root.join("accepted-state.bin")).unwrap(),
                    accepted_bytes()
                );
                log(
                    &root,
                    "caller.log",
                    "decision=UNKNOWN edit_calls=1 retry_calls=0",
                );
                let mount = std::ffi::CString::new(root.join("mnt").to_str().unwrap()).unwrap();
                assert_eq!(
                    unsafe { libc::umount2(mount.as_ptr(), libc::MNT_DETACH) },
                    0
                );
                assert!(!mounted(&root));
                println!("PROBE_CASE {case} PASS");
                return;
            }
            _ => panic!("unknown case {case}"),
        }
        drop(file);
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        assert!(!mounted(&root));
        println!("PROBE_CASE {case} PASS");
    }
}
