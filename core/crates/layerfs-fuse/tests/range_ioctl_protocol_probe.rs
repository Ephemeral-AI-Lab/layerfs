//! Test-only mounted v2 STATE/EDIT/ACK ioctl protocol; no Workspace or Store.
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

    const STATE: u32 = 0xc040_f542;
    const EDIT: u32 = 0xd040_f543;
    const ACK: u32 = 0xc040_f544;
    const INC: u64 = 0x2410_0000_0000_0001;
    const GEN: u64 = 7;
    const TTL: Duration = Duration::from_secs(60);
    const REPLY_LEN: usize = 48;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Stamp {
        incarnation: u64,
        generation: u64,
        revision: u64,
    }
    const INITIAL: Stamp = Stamp {
        incarnation: INC,
        generation: GEN,
        revision: 3,
    };
    const EDITED: Stamp = Stamp {
        incarnation: INC,
        generation: GEN,
        revision: 4,
    };
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Response {
        status: u16,
        nonce: u64,
        stamp: Stamp,
        size: u64,
    }

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
    fn stamp_at(bytes: &[u8]) -> Stamp {
        Stamp {
            incarnation: u64_at(bytes, 16),
            generation: u64_at(bytes, 24),
            revision: u64_at(bytes, 32),
        }
    }
    fn put_stamp(bytes: &mut [u8], stamp: Stamp) {
        bytes[16..24].copy_from_slice(&stamp.incarnation.to_le_bytes());
        bytes[24..32].copy_from_slice(&stamp.generation.to_le_bytes());
        bytes[32..40].copy_from_slice(&stamp.revision.to_le_bytes());
    }
    fn request(op: u16, nonce: u64, stamp: Option<Stamp>) -> Vec<u8> {
        let mut bytes = vec![0; if op == 2 { 4160 } else { 64 }];
        bytes[..4].copy_from_slice(b"LFR2");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&op.to_le_bytes());
        bytes[8..16].copy_from_slice(&nonce.to_le_bytes());
        if let Some(stamp) = stamp {
            put_stamp(&mut bytes, stamp);
        }
        if op == 2 {
            bytes[40..48].copy_from_slice(&4093u64.to_le_bytes());
            bytes[56..60].copy_from_slice(&4096u32.to_le_bytes());
            for (i, byte) in bytes[64..].iter_mut().enumerate() {
                *byte = (i % 239) as u8;
            }
        }
        bytes
    }
    fn encode(response: Response) -> [u8; REPLY_LEN] {
        let mut bytes = [0; REPLY_LEN];
        bytes[..4].copy_from_slice(b"LFA2");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&response.status.to_le_bytes());
        bytes[8..16].copy_from_slice(&response.nonce.to_le_bytes());
        put_stamp(&mut bytes, response.stamp);
        bytes[40..48].copy_from_slice(&response.size.to_le_bytes());
        bytes
    }
    fn decode(bytes: &[u8]) -> Response {
        assert!(bytes.len() >= REPLY_LEN && bytes[..4] == *b"LFA2" && u16_at(bytes, 4) == 2);
        Response {
            status: u16_at(bytes, 6),
            nonce: u64_at(bytes, 8),
            stamp: stamp_at(bytes),
            size: u64_at(bytes, 40),
        }
    }
    fn accepted_bytes() -> Vec<u8> {
        let mut bytes: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        bytes.splice(4093..4093, (0..4096).map(|i| (i % 239) as u8));
        bytes
    }

    struct State {
        stamp: Stamp,
        size: u64,
        mtime: u64,
        accepted: Option<(u64, bool)>,
        publications: u64,
    }
    struct Probe {
        root: PathBuf,
        case: String,
        state: Arc<Mutex<State>>,
        notifier: Arc<Mutex<Option<fuser::Notifier>>>,
    }
    impl Probe {
        fn attr(&self, ino: u64) -> FileAttr {
            let s = self.state.lock().unwrap();
            let dir = ino == 1;
            let at = UNIX_EPOCH + Duration::from_secs(s.mtime);
            FileAttr {
                ino: INodeNo(ino),
                size: if dir { 0 } else { s.size },
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
        fn respond(&self, reply: ReplyIoctl, response: Response) {
            let bytes = encode(response);
            log(
                &self.root,
                "events.log",
                &format!("reply bytes={} hex={}", bytes.len(), hex(&bytes)),
            );
            reply.ioctl(0, &bytes);
            log(
                &self.root,
                "events.log",
                "reply method returned (delivery not acknowledged to daemon)",
            );
        }
        fn refuse(&self, reply: ReplyIoctl, errno: Errno, why: &str) {
            log(
                &self.root,
                "events.log",
                &format!("reply errno={errno:?} reason={why}"),
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
            let len = self.state.lock().unwrap().size;
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
            let op = match cmd {
                STATE => 1,
                EDIT => 2,
                ACK => 3,
                _ => {
                    self.refuse(reply, Errno::ENOTTY, "command");
                    return;
                }
            };
            let expected_len = if op == 2 { 4160 } else { 64 };
            if input.len() != expected_len
                || out_size != expected_len as u32
                || input[..4] != *b"LFR2"
                || u16_at(input, 4) != 2
                || u16_at(input, 6) != op
                || u32_at(input, 60) != 0
            {
                self.refuse(reply, Errno::EINVAL, "header-or-length");
                return;
            }
            let nonce = u64_at(input, 8);
            if op == 1 {
                if input[16..].iter().any(|&b| b != 0) {
                    self.refuse(reply, Errno::EINVAL, "state-extra");
                    return;
                }
                let s = self.state.lock().unwrap();
                let status = if nonce == 0 {
                    0
                } else if let Some((known, confirmed)) = s.accepted {
                    if nonce != known {
                        self.refuse(reply, Errno::ENOENT, "nonce");
                        return;
                    }
                    if confirmed {
                        2
                    } else {
                        1
                    }
                } else {
                    self.refuse(reply, Errno::ENOENT, "nonce");
                    return;
                };
                let response = Response {
                    status,
                    nonce,
                    stamp: s.stamp,
                    size: s.size,
                };
                drop(s);
                self.respond(reply, response);
                return;
            }
            let stamp = stamp_at(input);
            if op == 3 {
                if input[40..].iter().any(|&b| b != 0) {
                    self.refuse(reply, Errno::EINVAL, "ack-extra");
                    return;
                }
                let mut s = self.state.lock().unwrap();
                if s.accepted.map(|(n, _)| n) != Some(nonce) {
                    self.refuse(reply, Errno::ENOENT, "ack-nonce");
                    return;
                }
                if stamp != s.stamp {
                    self.refuse(reply, Errno::ESTALE, "ack-stamp");
                    return;
                }
                s.accepted = Some((nonce, true));
                let response = Response {
                    status: 2,
                    nonce,
                    stamp: s.stamp,
                    size: s.size,
                };
                drop(s);
                self.respond(reply, response);
                return;
            }
            if nonce == 0
                || u64_at(input, 40) != 4093
                || u64_at(input, 48) != 0
                || u32_at(input, 56) != 4096
                || input[64..]
                    .iter()
                    .enumerate()
                    .any(|(i, &b)| b != (i % 239) as u8)
            {
                self.refuse(reply, Errno::EINVAL, "edit-shape");
                return;
            }
            let mut s = self.state.lock().unwrap();
            if s.accepted.map(|(n, _)| n) == Some(nonce) {
                self.refuse(reply, Errno::EALREADY, "duplicate-nonce");
                return;
            }
            if stamp != s.stamp {
                self.refuse(reply, Errno::ESTALE, "stale-stamp");
                return;
            }
            if s.accepted.is_some() {
                self.refuse(reply, Errno::EALREADY, "single-edit-fixture");
                return;
            }
            s.size = 12288;
            s.mtime += 1;
            s.stamp.revision += 1;
            s.accepted = Some((nonce, false));
            s.publications += 1;
            let response = Response {
                status: 1,
                nonce,
                stamp: s.stamp,
                size: s.size,
            };
            log(
                &self.root,
                "events.log",
                &format!(
                    "published nonce={nonce} revision={} size={} publications={}",
                    s.stamp.revision, s.size, s.publications
                ),
            );
            drop(s);
            let notify =
                self.notifier
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .inval_inode(INodeNo(2), 0, 0);
            log(
                &self.root,
                "events.log",
                &format!("inval_inode result={notify:?}"),
            );
            if notify.is_err() {
                self.refuse(reply, Errno::EIO, "notify-after-publication");
                return;
            }
            if self.case == "post_error_query" {
                self.refuse(reply, Errno::EIO, "test-only-post-publication-error");
            } else {
                self.respond(reply, response);
            }
        }
    }

    fn call(
        root: &Path,
        file: &File,
        name: &str,
        cmd: u32,
        mut bytes: Vec<u8>,
    ) -> io::Result<Response> {
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
                    "return={name} rc={rc} reply_len_observed=48 reply_hex={}",
                    hex(&bytes[..REPLY_LEN])
                ),
            );
            Ok(decode(&bytes))
        }
    }
    fn read(file: &File, at: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0; len];
        let n = file.read_at(&mut out, at).unwrap();
        out.truncate(n);
        out
    }
    fn confirm(root: &Path, file: &File) {
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
        assert_eq!((stat.len(), stat.mtime()), (12288, 1_700_000_001));
        assert_eq!(boundary, accepted_bytes()[4090..4106]);
        assert!(eof.is_empty());
    }
    fn mounted(root: &Path) -> bool {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| line.contains(&root.join("mnt").display().to_string()))
    }
    fn daemon(case: &str, root: &Path) {
        let state = Arc::new(Mutex::new(State {
            stamp: INITIAL,
            size: 8192,
            mtime: 1_700_000_000,
            accepted: None,
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
            MountOption::FSName("layerfs-241-protocol".into()),
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
                "final publications={} revision={} size={} accepted={:?}",
                s.publications, s.stamp.revision, s.size, s.accepted
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
        let initial = Response {
            status: 0,
            nonce: 0,
            stamp: INITIAL,
            size: 8192,
        };
        assert_eq!(
            call(&root, &file, "state-initial", STATE, request(1, 0, None)).unwrap(),
            initial
        );
        if case != "state" {
            let nonce = if case == "edit_ack" { 0x91 } else { 0x92 };
            let edit = call(&root, &file, "edit", EDIT, request(2, nonce, Some(INITIAL)));
            if case == "post_error_query" {
                assert_eq!(edit.unwrap_err().raw_os_error(), Some(libc::EIO));
                assert_eq!(
                    call(
                        &root,
                        &file,
                        "query-accepted",
                        STATE,
                        request(1, nonce, None)
                    )
                    .unwrap(),
                    Response {
                        status: 1,
                        nonce,
                        stamp: EDITED,
                        size: 12288
                    }
                );
            } else {
                assert_eq!(
                    edit.unwrap(),
                    Response {
                        status: 1,
                        nonce,
                        stamp: EDITED,
                        size: 12288
                    }
                );
            }
            confirm(&root, &file);
            assert_eq!(
                call(&root, &file, "ack", ACK, request(3, nonce, Some(EDITED))).unwrap(),
                Response {
                    status: 2,
                    nonce,
                    stamp: EDITED,
                    size: 12288
                }
            );
            assert_eq!(
                call(
                    &root,
                    &file,
                    "query-confirmed",
                    STATE,
                    request(1, nonce, None)
                )
                .unwrap(),
                Response {
                    status: 2,
                    nonce,
                    stamp: EDITED,
                    size: 12288
                }
            );
            if case == "edit_ack" {
                assert_eq!(
                    call(
                        &root,
                        &file,
                        "duplicate",
                        EDIT,
                        request(2, nonce, Some(INITIAL))
                    )
                    .unwrap_err()
                    .raw_os_error(),
                    Some(libc::EALREADY)
                );
                assert_eq!(
                    call(&root, &file, "stale", EDIT, request(2, 0x93, Some(INITIAL)))
                        .unwrap_err()
                        .raw_os_error(),
                    Some(libc::ESTALE)
                );
            }
        }
        log(&root, "caller.log", "retry_edit_calls=0");
        drop(file);
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        assert!(!mounted(&root));
        println!("PROBE_CASE {case} PASS");
    }
}
