//! Standalone Linux FUSE ioctl carrier experiment. No Workspace or Store calls.
#[cfg(target_os = "linux")]
mod linux {
    use fuser::{
        Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
        IoctlFlags, KernelConfig, MountOption, Notifier, OpenFlags, ReplyAttr, ReplyData,
        ReplyEntry, ReplyIoctl, ReplyOpen, ReplyWrite, Request, Session,
    };
    use layerfs_telemetry::{
        output::{Identity, OutputConfig, OutputMode},
        runtime::{Configuration, MonitorConfig, Runtime},
    };
    use std::{
        collections::{BTreeMap, HashMap},
        ffi::OsStr,
        fs::{self, File, OpenOptions},
        io::{self, Read},
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
    const HANDLE: u64 = 40;

    #[derive(Default)]
    struct Counts {
        ioctl: u64,
        read: u64,
        read_bytes: u64,
        write: u64,
        write_bytes: u64,
    }
    struct State {
        base_len: u64,
        size: u64,
        splice: Option<(u64, u64, Vec<u8>)>,
        writes: BTreeMap<u64, u8>,
        mtime: u64,
        next_handle: u64,
        handles: HashMap<u64, bool>,
        counts: Counts,
        events: Vec<String>,
    }
    impl State {
        fn new(size: u64) -> Self {
            Self {
                base_len: size,
                size,
                splice: None,
                writes: BTreeMap::new(),
                mtime: 1_700_000_000,
                next_handle: HANDLE,
                handles: HashMap::new(),
                counts: Counts::default(),
                events: Vec::new(),
            }
        }
        fn byte(&self, at: u64) -> u8 {
            if let Some(&v) = self.writes.get(&at) {
                return v;
            }
            if let Some((offset, deleted, inserted)) = &self.splice {
                if at >= *offset {
                    let rel = at - offset;
                    if rel < inserted.len() as u64 {
                        return inserted[rel as usize];
                    }
                    return ((at - inserted.len() as u64 + deleted) % 251) as u8;
                }
            }
            (at % 251) as u8
        }
        fn attr(&self, ino: u64) -> FileAttr {
            let directory = ino == 1;
            let at = UNIX_EPOCH + Duration::from_secs(self.mtime);
            FileAttr {
                ino: INodeNo(ino),
                size: if directory { 0 } else { self.size },
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
                nlink: if directory { 2 } else { 2 },
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                flags: 0,
            }
        }
    }
    struct Probe {
        state: Arc<Mutex<State>>,
        notifier: Arc<Mutex<Option<Notifier>>>,
        root: PathBuf,
        telemetry: Runtime,
    }
    impl Filesystem for Probe {
        fn init(&mut self, _req: &Request, config: &mut KernelConfig) -> io::Result<()> {
            self.state
                .lock()
                .unwrap()
                .events
                .push(format!("init {:?}", config.kernel_abi()));
            Ok(())
        }
        fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
            if parent.0 == 1 && (name == "file" || name == "alias") {
                reply.entry(&TTL, &self.state.lock().unwrap().attr(2), Generation(1));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
            if ino.0 == 1 || ino.0 == 2 {
                reply.attr(&TTL, &self.state.lock().unwrap().attr(ino.0));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
            if ino.0 != 2 {
                reply.error(Errno::ENOENT);
                return;
            }
            let mut s = self.state.lock().unwrap();
            let fh = s.next_handle;
            s.next_handle += 1;
            s.handles
                .insert(fh, flags.0 & libc::O_ACCMODE != libc::O_RDONLY);
            reply.opened(FileHandle(fh), FopenFlags::FOPEN_DIRECT_IO);
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
            let mut s = self.state.lock().unwrap();
            if ino.0 != 2 || !s.handles.contains_key(&fh.0) {
                reply.error(Errno::EBADF);
                return;
            }
            let end = s.size.min(offset.saturating_add(size as u64));
            let data: Vec<u8> = (offset..end).map(|p| s.byte(p)).collect();
            s.counts.read += 1;
            s.counts.read_bytes += data.len() as u64;
            reply.data(&data);
        }
        fn write(
            &self,
            _req: &Request,
            ino: INodeNo,
            fh: FileHandle,
            offset: u64,
            data: &[u8],
            _write_flags: fuser::WriteFlags,
            _flags: OpenFlags,
            _lock_owner: Option<fuser::LockOwner>,
            reply: ReplyWrite,
        ) {
            let (result, report) = self
                .telemetry
                .recorder()
                .run(4, "carrier.daemon.write", |_| {
                    let mut s = self.state.lock().unwrap();
                    if ino.0 != 2 || s.handles.get(&fh.0) != Some(&true) {
                        reply.error(Errno::EBADF);
                        return Ok::<(), ()>(());
                    }
                    let Some(end) = offset.checked_add(data.len() as u64) else {
                        reply.error(Errno::EINVAL);
                        return Ok(());
                    };
                    for (i, byte) in data.iter().enumerate() {
                        s.writes.insert(offset + i as u64, *byte);
                    }
                    s.size = s.size.max(end);
                    s.mtime += 1;
                    s.counts.write += 1;
                    s.counts.write_bytes += data.len() as u64;
                    reply.written(data.len() as u32);
                    Ok(())
                });
            assert!(result.is_ok());
            self.telemetry.publish(report);
        }
        fn ioctl(
            &self,
            _req: &Request,
            ino: INodeNo,
            fh: FileHandle,
            _flags: IoctlFlags,
            cmd: u32,
            input: &[u8],
            out_size: u32,
            reply: ReplyIoctl,
        ) {
            let (result, report) = self
                .telemetry
                .recorder()
                .run(3, "carrier.daemon.ioctl", |_| {
                    let mut s = self.state.lock().unwrap();
                    s.counts.ioctl += 1;
                    s.events.push(format!(
                        "ioctl cmd={cmd:#x} ino={} fh={} in={} out={} hex={}",
                        ino.0,
                        fh.0,
                        input.len(),
                        out_size,
                        hex(input)
                    ));
                    let err = if cmd != CMD {
                        Some(Errno::ENOTTY)
                    } else if ino.0 != 2
                        || !s.handles.contains_key(&fh.0)
                        || self.root.join("stale").exists()
                        || s.handles.get(&fh.0) == Some(&false)
                    {
                        Some(Errno::EBADF)
                    } else if input.len() != SIZE
                        || input[..4] != *b"LFR1"
                        || u16::from_le_bytes(input[4..6].try_into().unwrap()) != 1
                        || input[6..8] != [0, 0]
                        || input[28..32] != [0; 4]
                    {
                        Some(Errno::EINVAL)
                    } else {
                        None
                    };
                    if let Some(e) = err {
                        reply.error(e);
                        return Ok::<(), ()>(());
                    }
                    let offset = u64::from_le_bytes(input[8..16].try_into().unwrap());
                    let deleted = u64::from_le_bytes(input[16..24].try_into().unwrap());
                    let inserted = u32::from_le_bytes(input[24..28].try_into().unwrap()) as usize;
                    if inserted > 4096
                        || input[32 + inserted..].iter().any(|&b| b != 0)
                        || offset > s.size
                        || deleted > s.size - offset
                        || s.size - deleted > u64::MAX - inserted as u64
                        || s.splice.is_some()
                    {
                        reply.error(Errno::EINVAL);
                        return Ok(());
                    }
                    let payload = input[32..32 + inserted].to_vec();
                    s.size = s.size - deleted + inserted as u64;
                    s.splice = Some((offset, deleted, payload));
                    s.mtime += 1;
                    s.events.push("published".into());
                    drop(s);
                    if self.root.join("fail_notify").exists() {
                        self.state
                            .lock()
                            .unwrap()
                            .events
                            .push("injected-notification-failure uncertain".into());
                        reply.error(Errno::EIO);
                        return Ok(());
                    }
                    let notification = self.notifier.lock().unwrap().as_ref().unwrap().inval_inode(
                        INodeNo(2),
                        0,
                        0,
                    );
                    self.state
                        .lock()
                        .unwrap()
                        .events
                        .push(format!("inval_inode {notification:?}"));
                    if notification.is_err() {
                        reply.error(Errno::EIO);
                    } else {
                        reply.ioctl(0, &[]);
                    }
                    Ok(())
                });
            assert!(result.is_ok());
            self.telemetry.publish(report);
        }
    }
    fn hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02x}")).collect()
    }
    fn request(offset: u64, deleted: u64, inserted: usize) -> Vec<u8> {
        let mut b = vec![0; SIZE];
        b[..4].copy_from_slice(b"LFR1");
        b[4..6].copy_from_slice(&1u16.to_le_bytes());
        b[8..16].copy_from_slice(&offset.to_le_bytes());
        b[16..24].copy_from_slice(&deleted.to_le_bytes());
        b[24..28].copy_from_slice(&(inserted as u32).to_le_bytes());
        for (i, byte) in b[32..32 + inserted].iter_mut().enumerate() {
            *byte = (i % 239) as u8;
        }
        b
    }
    fn runtime(root: &Path, role: u8) -> Runtime {
        let mut output = OutputConfig::forward();
        output.mode = OutputMode::Local;
        output.directory = Some(root.to_path_buf());
        Runtime::start(Configuration {
            enabled: true,
            timing: true,
            monitor: MonitorConfig {
                cpu: true,
                memory: true,
                interval_ms: 10,
                history: 64,
                windows: 4,
            },
            output,
            identity: Identity {
                run: 241,
                pid: std::process::id(),
                role,
                namespace: 241,
            },
        })
        .unwrap()
    }
    fn ioctl(file: &File, cmd: u32, bytes: &mut [u8], rt: &Runtime) -> io::Result<()> {
        let (result, report) = rt.recorder().run(1, "carrier.ioctl", |_| {
            let rc = unsafe { libc::ioctl(file.as_raw_fd(), cmd as _, bytes.as_mut_ptr()) };
            if rc < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
        rt.publish(report);
        result
    }
    fn read(file: &File, offset: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0; len];
        let n = file.read_at(&mut out, offset).unwrap();
        out.truncate(n);
        out
    }
    fn run_case(case: &str, root: &Path, rt: &Runtime) {
        let path = root.join("mnt/file");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let initial = file.metadata().unwrap();
        let edit_offset = if initial.len() > 8192 {
            initial.len() / 2
        } else {
            4093
        };
        let mut req = request(edit_offset, 0, 4096);
        match case {
            "insert" | "delete" | "overwrite" => {
                if case == "delete" {
                    req = request(4093, 4096, 0);
                }
                if case == "overwrite" {
                    req = request(4093, 4096, 4096);
                }
                ioctl(&file, CMD, &mut req, rt).unwrap();
                let expected = 8192 - if case == "insert" { 0 } else { 4096 }
                    + if case == "delete" { 0 } else { 4096 };
                assert_eq!(file.metadata().unwrap().len(), expected);
            }
            "refusals" => {
                let mut tests = Vec::new();
                let mut v = req.clone();
                v[4] = 2;
                tests.push(("version", CMD, v, libc::EINVAL));
                let mut v = req.clone();
                v[24..28].copy_from_slice(&4097u32.to_le_bytes());
                tests.push(("length", CMD, v, libc::EINVAL));
                let mut v = req.clone();
                v[6] = 1;
                tests.push(("flags", CMD, v, libc::EINVAL));
                tests.push(("overflow", CMD, request(u64::MAX, 0, 4096), libc::EINVAL));
                tests.push(("unknown", CMD ^ 1, req.clone(), libc::ENOTTY));
                for (name, cmd, mut bytes, errno) in tests {
                    assert_eq!(
                        ioctl(&file, cmd, &mut bytes, rt)
                            .unwrap_err()
                            .raw_os_error(),
                        Some(errno),
                        "{name}"
                    );
                }
                let readonly = File::open(&path).unwrap();
                assert_eq!(
                    ioctl(&readonly, CMD, &mut req, rt)
                        .unwrap_err()
                        .raw_os_error(),
                    Some(libc::EBADF)
                );
                fs::write(root.join("stale"), b"1").unwrap();
                assert_eq!(
                    ioctl(&file, CMD, &mut req, rt).unwrap_err().raw_os_error(),
                    Some(libc::EBADF)
                );
                assert_eq!(
                    (
                        file.metadata().unwrap().len(),
                        file.metadata().unwrap().mtime()
                    ),
                    (initial.len(), initial.mtime())
                );
            }
            "coherence" => {
                let second = File::open(&path).unwrap();
                let alias = File::open(root.join("mnt/alias")).unwrap();
                assert_eq!(
                    second.metadata().unwrap().ino(),
                    alias.metadata().unwrap().ino()
                );
                ioctl(&file, CMD, &mut req, rt).unwrap();
                for fd in [&file, &second, &alias] {
                    let stat = fd.metadata().unwrap();
                    assert_eq!(stat.len(), 12288);
                    assert!(stat.mtime() > initial.mtime());
                    let mut expected =
                        vec![(4090 % 251) as u8, (4091 % 251) as u8, (4092 % 251) as u8];
                    expected.extend((0..13).map(|i| (i % 239) as u8));
                    assert_eq!(read(fd, 4090, 16), expected);
                    assert_eq!(read(fd, 12288, 16), b"");
                }
                file.write_all_at(b"LATER", 12283).unwrap();
                assert_eq!(read(&alias, 12280, 8)[3..], *b"LATER");
            }
            "uncertain" => {
                fs::write(root.join("fail_notify"), b"1").unwrap();
                assert_eq!(
                    ioctl(&file, CMD, &mut req, rt).unwrap_err().raw_os_error(),
                    Some(libc::EIO)
                );
                // The daemon log must retain publication and uncertain custody.
            }
            name if name.starts_with("ioctl-") => {
                ioctl(&file, CMD, &mut req, rt).unwrap();
            }
            name if name.starts_with("write-") => {
                let bytes = vec![0x5a; 4096];
                let (result, report) = rt
                    .recorder()
                    .run(2, "carrier.write", |_| file.write_at(&bytes, edit_offset));
                rt.publish(report);
                assert_eq!(result.unwrap(), 4096);
            }
            _ => panic!("unknown case {case}"),
        }
    }
    #[test]
    #[ignore = "requires privileged Linux Docker FUSE"]
    fn probe() {
        let case = std::env::var("PROBE_CASE").unwrap();
        let root = PathBuf::from(std::env::var("PROBE_ROOT").unwrap());
        if std::env::var_os("PROBE_DAEMON").is_some() {
            let length = case
                .split('-')
                .nth(1)
                .and_then(|s| s.strip_suffix('m'))
                .and_then(|s| s.parse::<u64>().ok())
                .map(|m| m * 1024 * 1024)
                .unwrap_or(8192);
            let state = Arc::new(Mutex::new(State::new(length)));
            let notifier = Arc::new(Mutex::new(None));
            let daemon_rt = runtime(&root.join("telemetry-daemon"), 2);
            let fs = Probe {
                state: state.clone(),
                notifier: notifier.clone(),
                root: root.clone(),
                telemetry: daemon_rt.clone(),
            };
            let mut config = Config::default();
            config.mount_options = vec![
                MountOption::RW,
                MountOption::NoSuid,
                MountOption::NoDev,
                MountOption::DefaultPermissions,
                MountOption::Exec,
                MountOption::NoAtime,
                MountOption::FSName("layerfs".into()),
                MountOption::Subtype("layerfs".into()),
                MountOption::CUSTOM("max_read=1048576".into()),
            ];
            let session = Session::new(fs, root.join("mnt"), &config).unwrap();
            let bg = session.spawn().unwrap();
            *notifier.lock().unwrap() = Some(bg.notifier());
            if let Some(line) = fs::read_to_string("/proc/self/mountinfo")
                .unwrap()
                .lines()
                .find(|line| line.contains(&root.join("mnt").display().to_string()))
            {
                fs::write(root.join("mountinfo"), line).unwrap();
            }
            fs::write(root.join("ready"), b"1").unwrap();
            let mut byte = [0];
            let _ = io::stdin().read(&mut byte);
            bg.umount_and_join().unwrap();
            let s = state.lock().unwrap();
            let log = format!("base_len={} size={} mtime={} ioctl={} read={} read_bytes={} write={} write_bytes={}\n{}\n",
                s.base_len, s.size, s.mtime, s.counts.ioctl, s.counts.read, s.counts.read_bytes,
                s.counts.write, s.counts.write_bytes, s.events.join("\n"));
            fs::write(root.join("daemon.log"), log).unwrap();
            drop(daemon_rt);
            return;
        }
        fs::create_dir_all(root.join("mnt")).unwrap();
        let exe = std::env::current_exe().unwrap();
        let mut child = Command::new(exe)
            .args(["--ignored", "--exact", "linux::probe", "--nocapture"])
            .env("PROBE_DAEMON", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
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
        assert!(root.join("ready").exists(), "mount did not become ready");
        let caller_rt = runtime(&root.join("telemetry-caller"), 1);
        run_case(&case, &root, &caller_rt);
        drop(caller_rt);
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        println!("PROBE_CASE {case} PASS");
    }
}
