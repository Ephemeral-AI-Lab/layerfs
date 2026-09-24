//! Test-only mounted ioctl CPU observation. Run ignored test inside privileged Linux Docker.
#[cfg(target_os = "linux")]
mod linux {
    use fuser::{
        Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
        IoctlFlags, KernelConfig, MountOption, Notifier, OpenFlags, ReplyAttr, ReplyEntry,
        ReplyIoctl, ReplyOpen, Request, Session,
    };
    use layerfs_telemetry::{
        observation::{Observation, Source, Window, WindowSource},
        operation::{Diagnostic, OperationRecorder},
        output::{encode_operation, Identity},
        timer::RecordingLimits,
    };
    use std::{
        ffi::OsStr,
        fs::{self, File, OpenOptions},
        io::{self, Read},
        os::fd::AsRawFd,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant, UNIX_EPOCH},
    };

    const SIZE: usize = 4128;
    const CMD: u32 = (1 << 30) | ((SIZE as u32) << 16) | (0xf5 << 8) | 0x41;
    const TTL: Duration = Duration::from_secs(60);

    #[derive(Clone, Copy)]
    struct Sample {
        observation: Observation,
        thread_ns: u64,
    }
    struct Boundaries {
        origin: Instant,
        first: Mutex<Option<Sample>>,
        thread_delta: Mutex<Option<u64>>,
    }
    impl Boundaries {
        fn new() -> Self {
            Self {
                origin: Instant::now(),
                first: Mutex::new(None),
                thread_delta: Mutex::new(None),
            }
        }
        fn sample(&self, opening: bool) -> Option<Sample> {
            let micros = |v: libc::timeval| {
                u64::try_from(v.tv_sec)
                    .ok()?
                    .checked_mul(1_000_000)?
                    .checked_add(u64::try_from(v.tv_usec).ok()?)?
                    .checked_mul(1000)
            };
            let rss = || {
                let statm = fs::read_to_string("/proc/self/statm").ok()?;
                let pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
                let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
                pages.checked_mul(u64::try_from(page).ok()?)
            };
            let cpu = || {
                let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
                if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
                    return None;
                }
                let usage = unsafe { usage.assume_init() };
                Some((micros(usage.ru_utime)?, micros(usage.ru_stime)?))
            };
            let thread = || {
                let mut clock = std::mem::MaybeUninit::<libc::timespec>::uninit();
                if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, clock.as_mut_ptr()) }
                    != 0
                {
                    return None;
                }
                let clock = unsafe { clock.assume_init() };
                u64::try_from(clock.tv_sec)
                    .ok()?
                    .checked_mul(1_000_000_000)?
                    .checked_add(u64::try_from(clock.tv_nsec).ok()?)
            };
            // The thread clock is nearest the timer on both sides. Procfs stays outside.
            let (rss, (user_ns, system_ns), thread_ns) = if opening {
                (rss()?, cpu()?, thread()?)
            } else {
                let thread_ns = thread()?;
                let process_cpu = cpu()?;
                (rss()?, process_cpu, thread_ns)
            };
            Some(Sample {
                observation: Observation {
                    selected: 3,
                    source: Source::Supplied,
                    probe_ns: None,
                    at_ns: u64::try_from(self.origin.elapsed().as_nanos()).ok()?,
                    incarnation: std::process::id() as u64,
                    user_ns: Some(user_ns),
                    system_ns: Some(system_ns),
                    rss: Some(rss),
                },
                thread_ns,
            })
        }
        fn thread_delta(&self) -> Option<u64> {
            *self.thread_delta.lock().unwrap()
        }
    }
    impl WindowSource for Boundaries {
        fn begin(&self) -> Option<u64> {
            *self.first.lock().ok()? = Some(self.sample(true)?);
            Some(1)
        }
        fn finish(&self, token: u64) -> Option<Window> {
            if token != 1 {
                return None;
            }
            let second = self.sample(false)?;
            let first = self.first.lock().ok()?.take()?;
            *self.thread_delta.lock().ok()? = second.thread_ns.checked_sub(first.thread_ns);
            let mut window = Window {
                opened_ns: Some(first.observation.at_ns),
                closed_ns: Some(second.observation.at_ns),
                ..Window::default()
            };
            window.observe(first.observation, 1);
            window.observe(second.observation, 1);
            Some(window)
        }
        fn release(&self, _token: u64) {}
    }

    fn recorder() -> (OperationRecorder, Arc<Boundaries>) {
        let source = Arc::new(Boundaries::new());
        let recorder = OperationRecorder::new(RecordingLimits::new(8, 4, 8192).unwrap(), 1, 16384)
            .unwrap()
            .with_windows(source.clone());
        (recorder, source)
    }
    fn save(root: &Path, role: u8, diagnostic: Diagnostic, source: &Boundaries) {
        let Diagnostic::Report(report) = diagnostic else {
            panic!("LFT1 report omitted");
        };
        let identity = Identity {
            run: 242,
            pid: std::process::id(),
            role,
            namespace: 241,
        };
        let window = report.window().expect("resource window");
        let cpu = window.cpu_delta().expect("two valid CPU boundaries");
        let thread_cpu = source.thread_delta().expect("thread CPU boundaries");
        let line = encode_operation(identity, &report, 16384).expect("LFT1 encoding");
        let prefix = if role == 1 { "caller" } else { "daemon" };
        fs::write(root.join(format!("{prefix}.lft1")), line).unwrap();
        fs::write(
            root.join(format!("{prefix}.thread-cpu")),
            format!("thread_cpu_ns={thread_cpu} process_user_ns={} process_system_ns={} rss_begin_bytes={} rss_end_bytes={}\n",
                cpu.0, cpu.1, window.first.unwrap().rss.unwrap(), window.last.unwrap().rss.unwrap()),
        ).unwrap();
    }
    struct State {
        length: u64,
        ioctl: u64,
        input_bytes: u64,
        read: u64,
        write: u64,
        events: Vec<String>,
    }
    struct Probe {
        state: Arc<Mutex<State>>,
        notifier: Arc<Mutex<Option<Notifier>>>,
        root: PathBuf,
    }
    impl Probe {
        fn attr(&self, ino: u64) -> FileAttr {
            let is_dir = ino == 1;
            let stamp = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
            FileAttr {
                ino: INodeNo(ino),
                size: if is_dir {
                    0
                } else {
                    self.state.lock().unwrap().length
                },
                blocks: 0,
                atime: stamp,
                mtime: stamp,
                ctime: stamp,
                crtime: stamp,
                kind: if is_dir {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                },
                perm: if is_dir { 0o755 } else { 0o666 },
                nlink: 2,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                flags: 0,
            }
        }
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
            if parent.0 == 1 && name == "file" {
                reply.entry(&TTL, &self.attr(2), Generation(1));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
            if ino.0 == 1 || ino.0 == 2 {
                reply.attr(&TTL, &self.attr(ino.0));
            } else {
                reply.error(Errno::ENOENT);
            }
        }
        fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
            if ino.0 == 2 && flags.0 & libc::O_ACCMODE != libc::O_RDONLY {
                reply.opened(FileHandle(40), FopenFlags::FOPEN_DIRECT_IO);
            } else {
                reply.error(Errno::EBADF);
            }
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
            let (recorder, source) = recorder();
            let (result, report) = recorder.run(3, "carrier.daemon.ioctl", |_| {
                let mut state = self.state.lock().unwrap();
                state.ioctl += 1;
                state.input_bytes += input.len() as u64;
                let valid = cmd == CMD && ino.0 == 2 && fh.0 == 40 && out_size == 0
                    && input.len() == SIZE && input[..4] == *b"LFR1"
                    && input[4..8] == [1, 0, 0, 0]
                    && u32::from_le_bytes(input[24..28].try_into().unwrap()) == 4096
                    && input[28..32] == [0; 4];
                if !valid {
                    reply.error(Errno::EINVAL);
                    return Ok::<(), ()>(());
                }
                let offset = u64::from_le_bytes(input[8..16].try_into().unwrap());
                let deleted = u64::from_le_bytes(input[16..24].try_into().unwrap());
                if deleted != 0 || offset != state.length / 2 {
                    reply.error(Errno::EINVAL);
                    return Ok(());
                }
                state.length += 4096;
                state.events.push(format!("cmd={cmd:#x} ino={} fh={} in={} out={} offset={offset} deleted={deleted} inserted=4096", ino.0, fh.0, input.len(), out_size));
                drop(state);
                let notification = self.notifier.lock().unwrap().as_ref().unwrap().inval_inode(INodeNo(2), 0, 0);
                if notification.is_err() {
                    reply.error(Errno::EIO);
                } else {
                    reply.ioctl(0, &[]);
                }
                Ok(())
            });
            assert!(result.is_ok());
            save(&self.root, 2, report, &source);
        }
    }
    fn request(offset: u64) -> Vec<u8> {
        let mut bytes = vec![0; SIZE];
        bytes[..4].copy_from_slice(b"LFR1");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[8..16].copy_from_slice(&offset.to_le_bytes());
        bytes[24..28].copy_from_slice(&4096u32.to_le_bytes());
        for (index, byte) in bytes[32..].iter_mut().enumerate() {
            *byte = (index % 239) as u8;
        }
        bytes
    }
    fn caller(file: &File, offset: u64, root: &Path) {
        let mut bytes = request(offset);
        let (recorder, source) = recorder();
        let (result, report) = recorder.run(1, "carrier.caller.ioctl", |_| {
            let rc = unsafe { libc::ioctl(file.as_raw_fd(), CMD as _, bytes.as_mut_ptr()) };
            if rc < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
        save(root, 1, report, &source);
        result.unwrap();
    }
    #[test]
    #[ignore = "requires privileged Linux Docker FUSE"]
    fn probe() {
        let case = std::env::var("PROBE_CASE").unwrap();
        let root = PathBuf::from(std::env::var("PROBE_ROOT").unwrap());
        let initial = match case.as_str() {
            "ioctl-1m" => 1024 * 1024,
            "ioctl-500m" => 500 * 1024 * 1024,
            _ => panic!("unknown case {case}"),
        };
        if std::env::var_os("PROBE_DAEMON").is_some() {
            let state = Arc::new(Mutex::new(State {
                length: initial,
                ioctl: 0,
                input_bytes: 0,
                read: 0,
                write: 0,
                events: Vec::new(),
            }));
            let notifier = Arc::new(Mutex::new(None));
            let mut config = Config::default();
            config.mount_options = vec![
                MountOption::RW,
                MountOption::NoSuid,
                MountOption::NoDev,
                MountOption::DefaultPermissions,
                MountOption::NoAtime,
                MountOption::FSName("layerfs".into()),
                MountOption::Subtype("layerfs".into()),
            ];
            let fs = Probe {
                state: state.clone(),
                notifier: notifier.clone(),
                root: root.clone(),
            };
            let session = Session::new(fs, root.join("mnt"), &config).unwrap();
            let bg = session.spawn().unwrap();
            *notifier.lock().unwrap() = Some(bg.notifier());
            let mountinfo = fs::read_to_string("/proc/self/mountinfo").unwrap();
            fs::write(
                root.join("mountinfo"),
                mountinfo
                    .lines()
                    .find(|line| line.contains(&root.join("mnt").display().to_string()))
                    .unwrap(),
            )
            .unwrap();
            fs::write(root.join("ready"), b"1").unwrap();
            let mut byte = [0];
            let _ = io::stdin().read(&mut byte);
            bg.umount_and_join().unwrap();
            let state = state.lock().unwrap();
            fs::write(
                root.join("daemon.log"),
                format!(
                    "length={} ioctl={} input_bytes={} read={} write={}\n{}\n",
                    state.length,
                    state.ioctl,
                    state.input_bytes,
                    state.read,
                    state.write,
                    state.events.join("\n")
                ),
            )
            .unwrap();
            return;
        }
        fs::create_dir_all(root.join("mnt")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
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
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("mnt/file"))
            .unwrap();
        caller(&file, initial / 2, &root);
        assert_eq!(file.metadata().unwrap().len(), initial + 4096);
        drop(file);
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        println!("PROBE_CASE {case} PASS");
    }
}
