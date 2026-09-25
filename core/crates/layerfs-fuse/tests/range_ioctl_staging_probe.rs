//! Test-only live Linux/FUSE experiment for bounded, ordered ioctl staging.
#[cfg(target_os = "linux")]
mod linux {
    use fuser::{
        Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
        IoctlFlags, MountOption, OpenFlags, ReplyAttr, ReplyData, ReplyEmpty, ReplyEntry,
        ReplyIoctl, ReplyOpen, Request, Session,
    };
    use std::{
        ffi::OsStr,
        fs::{self, File, OpenOptions},
        io::{self, Read, Write},
        os::{fd::AsRawFd, unix::fs::FileExt},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Arc, Mutex},
        thread,
        time::{Duration, UNIX_EPOCH},
    };

    const BEGIN: u32 = 0xc080_f542;
    const DATA: u32 = 0x5080_f543;
    const APPLY: u32 = 0x4080_f544;
    const ABORT: u32 = 0x4080_f545;
    const TOKEN: u64 = 0x241232;
    const TTL: Duration = Duration::from_secs(60);
    const PAYLOAD_LEN: usize = 65536;
    // SHA-256 of the fixed 64 KiB carrier pattern. The probe checks delivery;
    // the product's digest implementation receives its own correctness tests.
    const DIGEST: [u8; 32] = [
        0xd2, 0x4a, 0x9d, 0x12, 0xe3, 0xaa, 0xca, 0x59, 0x74, 0xe6, 0x99, 0x1e, 0xe7, 0xf1, 0xab,
        0x38, 0xbd, 0xff, 0x19, 0x4f, 0xb6, 0x8b, 0x91, 0x89, 0xa1, 0xc2, 0x01, 0x2a, 0x64, 0xe1,
        0x7b, 0xaf,
    ];

    fn log(root: &Path, line: impl std::fmt::Display) {
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
    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }
    fn payload() -> Vec<u8> {
        (0..PAYLOAD_LEN).map(|i| (i % 239) as u8).collect()
    }
    fn base() -> Vec<u8> {
        (0..8192).map(|i| (i % 251) as u8).collect()
    }
    fn begin(revision: u64, digest: [u8; 32]) -> Vec<u8> {
        let mut b = vec![0; 128];
        b[..4].copy_from_slice(b"LFB3");
        b[4..6].copy_from_slice(&3u16.to_le_bytes());
        b[8..16].copy_from_slice(&revision.to_le_bytes());
        b[16..24].copy_from_slice(&(PAYLOAD_LEN as u64).to_le_bytes());
        b[24..56].copy_from_slice(&digest);
        b
    }
    fn data(offset: u64, bytes: &[u8]) -> Vec<u8> {
        let mut b = vec![0; 4224];
        b[..4].copy_from_slice(b"LFD3");
        b[4..6].copy_from_slice(&3u16.to_le_bytes());
        b[8..16].copy_from_slice(&TOKEN.to_le_bytes());
        b[16..24].copy_from_slice(&offset.to_le_bytes());
        b[24..28].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        b[128..128 + bytes.len()].copy_from_slice(bytes);
        b
    }
    fn terminal(magic: &[u8; 4]) -> Vec<u8> {
        let mut b = vec![0; 128];
        b[..4].copy_from_slice(magic);
        b[4..6].copy_from_slice(&3u16.to_le_bytes());
        b[8..16].copy_from_slice(&TOKEN.to_le_bytes());
        b
    }
    fn frame(size: usize) -> (u32, Vec<u8>) {
        let cmd = (1u32 << 30) | ((size as u32) << 16) | (0xf5 << 8) | 0x4f;
        (cmd, vec![0xa5; size])
    }
    struct Stage {
        fh: u64,
        next: usize,
        digest: [u8; 32],
        bytes: Vec<u8>,
    }
    struct State {
        bytes: Vec<u8>,
        revision: u64,
        publications: u64,
        next_fh: u64,
        stage: Option<Stage>,
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
            let time = UNIX_EPOCH + Duration::from_secs(1_700_000_000 + s.revision);
            FileAttr {
                ino: INodeNo(ino),
                size: if ino == 1 { 0 } else { s.bytes.len() as u64 },
                blocks: 0,
                atime: time,
                mtime: time,
                ctime: time,
                crtime: time,
                kind: if ino == 1 {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                },
                perm: if ino == 1 { 0o755 } else { 0o644 },
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                flags: 0,
            }
        }
    }
    impl Filesystem for Probe {
        fn lookup(&self, _: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
            if parent.0 == 1 && name == "file" {
                reply.entry(&TTL, &self.attr(2), Generation(1));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn getattr(&self, _: &Request, ino: INodeNo, _: Option<FileHandle>, reply: ReplyAttr) {
            if ino.0 <= 2 {
                reply.attr(&TTL, &self.attr(ino.0));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn open(&self, _: &Request, ino: INodeNo, _: OpenFlags, reply: ReplyOpen) {
            if ino.0 != 2 {
                reply.error(Errno::ENOENT);
                return;
            }
            let mut s = self.state.lock().unwrap();
            let fh = s.next_fh;
            s.next_fh += 1;
            log(&self.root, format!("open fh={fh}"));
            reply.opened(FileHandle(fh), FopenFlags::FOPEN_DIRECT_IO);
        }
        fn release(
            &self,
            _: &Request,
            _: INodeNo,
            fh: FileHandle,
            _: OpenFlags,
            _: Option<fuser::LockOwner>,
            _: bool,
            reply: ReplyEmpty,
        ) {
            let mut s = self.state.lock().unwrap();
            if s.stage.as_ref().is_some_and(|stage| stage.fh == fh.0) {
                s.stage = None;
                log(&self.root, format!("release fh={} stage=discarded", fh.0));
            } else {
                log(&self.root, format!("release fh={} stage=none", fh.0));
            }
            reply.ok();
        }
        fn read(
            &self,
            _: &Request,
            ino: INodeNo,
            _: FileHandle,
            offset: u64,
            size: u32,
            _: OpenFlags,
            _: Option<fuser::LockOwner>,
            reply: ReplyData,
        ) {
            if ino.0 != 2 {
                reply.error(Errno::EBADF);
                return;
            }
            let s = self.state.lock().unwrap();
            let start = (offset as usize).min(s.bytes.len());
            let end = start.saturating_add(size as usize).min(s.bytes.len());
            reply.data(&s.bytes[start..end]);
        }
        fn ioctl(
            &self,
            _: &Request,
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
                format!(
                    "callback cmd={cmd:#x} fh={} flags={flags:?} in={} out={out_size}",
                    fh.0,
                    input.len()
                ),
            );
            if ino.0 != 2 {
                reply.error(Errno::EBADF);
                return;
            }
            if cmd & 0xff == 0x4f {
                reply.ioctl(0, &[]);
                log(&self.root, "reply frame rc=0 bytes=0");
                return;
            }
            let mut s = self.state.lock().unwrap();
            let error = match cmd {
                BEGIN if input.len() == 128 && out_size == 128 && &input[..4] == b"LFB3" => {
                    if u64_at(input, 8) != s.revision {
                        Some(Errno::ESTALE)
                    } else if s.stage.is_some() {
                        Some(Errno::EBUSY)
                    } else if u64_at(input, 16) != PAYLOAD_LEN as u64 {
                        Some(Errno::EINVAL)
                    } else {
                        s.stage = Some(Stage {
                            fh: fh.0,
                            next: 0,
                            digest: input[24..56].try_into().unwrap(),
                            bytes: Vec::new(),
                        });
                        let mut response = vec![0; 128];
                        response[..4].copy_from_slice(b"LFB3");
                        response[8..16].copy_from_slice(&TOKEN.to_le_bytes());
                        reply.ioctl(0, &response);
                        log(&self.root, "reply BEGIN rc=0 bytes=128");
                        return;
                    }
                }
                DATA if input.len() == 4224 && out_size == 0 && &input[..4] == b"LFD3" => {
                    let offset = u64_at(input, 16) as usize;
                    let len = u32::from_le_bytes(input[24..28].try_into().unwrap()) as usize;
                    match &mut s.stage {
                        Some(stage) if stage.fh == fh.0 && u64_at(input, 8) == TOKEN => {
                            if offset != stage.next
                                || len == 0
                                || len > 4096
                                || offset + len > PAYLOAD_LEN
                            {
                                Some(Errno::EINVAL)
                            } else {
                                stage.bytes.extend_from_slice(&input[128..128 + len]);
                                stage.next += len;
                                reply.ioctl(0, &[]);
                                log(&self.root, format!("reply DATA rc=0 next={}", stage.next));
                                return;
                            }
                        }
                        _ => Some(Errno::EBADF),
                    }
                }
                APPLY if input.len() == 128 && out_size == 0 && &input[..4] == b"LFA3" => {
                    match s.stage.take() {
                        Some(stage) if stage.fh == fh.0 && u64_at(input, 8) == TOKEN => {
                            if stage.next != PAYLOAD_LEN
                                || stage.digest != DIGEST
                                || stage.bytes != payload()
                            {
                                Some(Errno::EINVAL)
                            } else {
                                s.bytes.splice(4096..4096, stage.bytes);
                                s.revision += 1;
                                s.publications += 1;
                                log(
                                    &self.root,
                                    format!(
                                        "published revision={} publications={}",
                                        s.revision, s.publications
                                    ),
                                );
                                if self.case == "lost_reply" {
                                    fs::write(
                                        self.root.join("accepted.sha256"),
                                        "published-before-reply",
                                    )
                                    .unwrap();
                                    std::process::exit(23);
                                }
                                drop(s);
                                let notified =
                                    self.notifier.lock().unwrap().as_ref().unwrap().inval_inode(
                                        INodeNo(2),
                                        0,
                                        0,
                                    );
                                log(&self.root, format!("notify result={notified:?}"));
                                reply.ioctl(0, &[]);
                                log(&self.root, "reply APPLY rc=0 bytes=0");
                                return;
                            }
                        }
                        _ => Some(Errno::EBADF),
                    }
                }
                ABORT if input.len() == 128 && out_size == 0 && &input[..4] == b"LFX3" => {
                    if s.stage
                        .as_ref()
                        .is_some_and(|stage| stage.fh == fh.0 && u64_at(input, 8) == TOKEN)
                    {
                        s.stage = None;
                        reply.ioctl(0, &[]);
                        log(&self.root, "reply ABORT rc=0 bytes=0");
                        return;
                    }
                    Some(Errno::EBADF)
                }
                _ => Some(Errno::EINVAL),
            };
            let error = error.unwrap();
            log(&self.root, format!("reply errno={error:?}"));
            reply.error(error);
        }
    }
    fn daemon(root: &Path, case: &str) {
        let state = Arc::new(Mutex::new(State {
            bytes: base(),
            revision: 0,
            publications: 0,
            next_fh: 40,
            stage: None,
        }));
        let notifier = Arc::new(Mutex::new(None));
        let mut config = Config::default();
        config.mount_options = vec![
            MountOption::RW,
            MountOption::DefaultPermissions,
            MountOption::FSName("layerfs-stage-probe".into()),
        ];
        let session = Session::new(
            Probe {
                root: root.into(),
                case: case.into(),
                state: state.clone(),
                notifier: notifier.clone(),
            },
            root.join("mnt"),
            &config,
        )
        .unwrap();
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
        if case == "unmount" {
            bg.join().unwrap();
        } else {
            bg.umount_and_join().unwrap();
        }
        let mut s = state.lock().unwrap();
        s.stage = None;
        log(
            root,
            format!(
                "final revision={} publications={} stage_present={} unmounted=true",
                s.revision,
                s.publications,
                s.stage.is_some()
            ),
        );
    }
    fn call(
        root: &Path,
        file: &File,
        label: &str,
        cmd: u32,
        mut bytes: Vec<u8>,
    ) -> io::Result<Vec<u8>> {
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), cmd as _, bytes.as_mut_ptr()) };
        let result = if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(bytes)
        };
        log(
            root,
            format!(
                "caller {label} cmd={cmd:#x} rc={rc} errno={:?}",
                result.as_ref().err().and_then(io::Error::raw_os_error)
            ),
        );
        result
    }
    fn send_payload(root: &Path, file: &File) {
        let bytes = payload();
        for (index, chunk) in bytes.chunks(4096).enumerate() {
            call(root, file, "DATA", DATA, data((index * 4096) as u64, chunk)).unwrap();
        }
    }
    fn read(file: &File, at: u64, len: usize) -> Vec<u8> {
        let mut bytes = vec![0; len];
        let n = file.read_at(&mut bytes, at).unwrap();
        bytes.truncate(n);
        bytes
    }
    #[test]
    #[ignore = "requires privileged Linux Docker FUSE"]
    fn probe() {
        let case = std::env::var("PROBE_CASE").unwrap();
        let root = PathBuf::from(std::env::var("PROBE_ROOT").unwrap());
        if std::env::var_os("PROBE_DAEMON").is_some() {
            daemon(&root, &case);
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
        let digest = DIGEST;
        match case.as_str() {
            "frames" => {
                for size in [4096, 8192, 12288, 16368] {
                    let (cmd, bytes) = frame(size);
                    call(&root, &file, &format!("frame-{size}"), cmd, bytes).unwrap();
                }
            }
            "success" => {
                let response = call(&root, &file, "BEGIN", BEGIN, begin(0, digest)).unwrap();
                assert_eq!(u64_at(&response, 8), TOKEN);
                assert_eq!(read(&file, 4090, 32), base()[4090..4122]);
                send_payload(&root, &file);
                assert_eq!(read(&file, 4090, 32), base()[4090..4122]);
                call(&root, &file, "APPLY", APPLY, terminal(b"LFA3")).unwrap();
                assert_eq!(read(&file, 4096, PAYLOAD_LEN), payload());
                assert_eq!(
                    read(&file, (8192 + PAYLOAD_LEN) as u64, 1),
                    Vec::<u8>::new()
                );
            }
            "wrong_order" => {
                call(&root, &file, "BEGIN", BEGIN, begin(0, digest)).unwrap();
                assert_eq!(
                    call(
                        &root,
                        &file,
                        "DATA wrong",
                        DATA,
                        data(4096, &payload()[..4096])
                    )
                    .unwrap_err()
                    .raw_os_error(),
                    Some(libc::EINVAL)
                );
                call(&root, &file, "ABORT", ABORT, terminal(b"LFX3")).unwrap();
            }
            "bad_digest" => {
                call(&root, &file, "BEGIN", BEGIN, begin(0, [0; 32])).unwrap();
                send_payload(&root, &file);
                assert_eq!(
                    call(&root, &file, "APPLY bad", APPLY, terminal(b"LFA3"))
                        .unwrap_err()
                        .raw_os_error(),
                    Some(libc::EINVAL)
                );
            }
            "stale_stamp" => {
                assert_eq!(
                    call(&root, &file, "BEGIN stale", BEGIN, begin(1, digest))
                        .unwrap_err()
                        .raw_os_error(),
                    Some(libc::ESTALE)
                );
            }
            "close" => {
                call(&root, &file, "BEGIN", BEGIN, begin(0, digest)).unwrap();
                drop(file);
                let reopened = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(root.join("mnt/file"))
                    .unwrap();
                assert_eq!(
                    call(
                        &root,
                        &reopened,
                        "DATA old token",
                        DATA,
                        data(0, &payload()[..4096])
                    )
                    .unwrap_err()
                    .raw_os_error(),
                    Some(libc::EBADF)
                );
                drop(reopened);
                drop(child.stdin.take());
                assert!(child.wait().unwrap().success());
                println!("STAGING_PROBE {case} PASS");
                return;
            }
            "abort" => {
                call(&root, &file, "BEGIN", BEGIN, begin(0, digest)).unwrap();
                call(&root, &file, "ABORT", ABORT, terminal(b"LFX3")).unwrap();
                assert_eq!(
                    call(&root, &file, "APPLY after abort", APPLY, terminal(b"LFA3"))
                        .unwrap_err()
                        .raw_os_error(),
                    Some(libc::EBADF)
                );
            }
            "unmount" => {
                call(&root, &file, "BEGIN", BEGIN, begin(0, digest)).unwrap();
                let mount = std::ffi::CString::new(root.join("mnt").to_str().unwrap()).unwrap();
                assert_eq!(
                    unsafe { libc::umount2(mount.as_ptr(), libc::MNT_DETACH) },
                    0
                );
                log(
                    &root,
                    "caller forced-detach while staged descriptor remains open",
                );
                drop(child.stdin.take());
                assert!(child.wait().unwrap().success());
                drop(file);
                println!("STAGING_PROBE {case} PASS");
                return;
            }
            "lost_reply" => {
                call(&root, &file, "BEGIN", BEGIN, begin(0, digest)).unwrap();
                send_payload(&root, &file);
                let error = call(&root, &file, "APPLY lost", APPLY, terminal(b"LFA3")).unwrap_err();
                assert!([libc::ECONNABORTED, libc::ENOTCONN, libc::EIO]
                    .contains(&error.raw_os_error().unwrap_or_default()));
                assert_eq!(child.wait().unwrap().code(), Some(23));
                assert!(root.join("accepted.sha256").exists());
                let mount = std::ffi::CString::new(root.join("mnt").to_str().unwrap()).unwrap();
                assert_eq!(
                    unsafe { libc::umount2(mount.as_ptr(), libc::MNT_DETACH) },
                    0
                );
                println!("STAGING_PROBE {case} PASS");
                return;
            }
            _ => panic!("unknown case {case}"),
        }
        let expected = if case == "success" {
            let mut bytes = base();
            bytes.splice(4096..4096, payload());
            bytes
        } else {
            base()
        };
        assert_eq!(read(&file, 0, expected.len()), expected);
        drop(file);
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        println!("STAGING_PROBE {case} PASS");
    }
}
