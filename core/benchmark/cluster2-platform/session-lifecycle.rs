//! External public Session lifecycle oracle, independent of LayerFS native service.
use fuser::{
    Config, FileAttr, FileType, Filesystem, INodeNo, ReplyAttr, Request, Session, SessionACL,
    SessionMonitor, SessionOutcome, SessionPhase,
};
use std::{
    fs::{File, OpenOptions},
    io,
    net::Shutdown,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::net::UnixDatagram,
    },
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

const WAIT: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct Probe {
    callbacks: Arc<AtomicUsize>,
    destroyed: Arc<AtomicUsize>,
    panic_getattr: bool,
    panic_destroy: bool,
}
impl Probe {
    fn new() -> Self {
        Self {
            callbacks: Arc::new(AtomicUsize::new(0)),
            destroyed: Arc::new(AtomicUsize::new(0)),
            panic_getattr: false,
            panic_destroy: false,
        }
    }
}
impl Filesystem for Probe {
    fn getattr(&self, _: &Request, ino: INodeNo, _: Option<fuser::FileHandle>, reply: ReplyAttr) {
        self.callbacks.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic_getattr, "original receiver panic");
        reply.attr(
            &Duration::ZERO,
            &FileAttr {
                ino,
                size: 0,
                blocks: 0,
                atime: UNIX_EPOCH,
                mtime: UNIX_EPOCH,
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                flags: 0,
            },
        );
    }
    fn destroy(&mut self) {
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic_destroy, "original destroy panic");
    }
}
fn u32_at(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_ne_bytes());
}
fn u64_at(b: &mut [u8], at: usize, value: u64) {
    b[at..at + 8].copy_from_slice(&value.to_ne_bytes());
}
fn frame(op: u32, id: u64, body: usize) -> Vec<u8> {
    let mut b = vec![0; 40 + body];
    let len = b.len() as u32;
    u32_at(&mut b, 0, len);
    u32_at(&mut b, 4, op);
    u64_at(&mut b, 8, id);
    u64_at(&mut b, 16, 1);
    b
}
fn reply(client: &UnixDatagram, id: u64) -> io::Result<()> {
    let mut b = [0; 512];
    let n = client.recv(&mut b)?;
    assert!(n >= 16);
    assert_eq!(u64::from_ne_bytes(b[8..16].try_into().unwrap()), id);
    assert_eq!(i32::from_ne_bytes(b[4..8].try_into().unwrap()), 0);
    Ok(())
}
fn prepared(
    fs: Probe,
    n: usize,
) -> io::Result<(fuser::SessionRunner<Probe>, UnixDatagram, UnixDatagram)> {
    let (client, server) = UnixDatagram::pair()?;
    let stop = server.try_clone()?;
    client.set_read_timeout(Some(WAIT))?;
    client.set_write_timeout(Some(WAIT))?;
    let mut init = frame(26, 1, 64);
    u32_at(&mut init, 40, 7);
    u32_at(&mut init, 44, 38);
    client.send(&init)?;
    let mut config = Config::default();
    config.n_threads = Some(n);
    config.clone_fd = false;
    let runner =
        Session::from_fd(fs, OwnedFd::from(server), SessionACL::All, config)?.into_runner();
    reply(&client, 1)?;
    assert_eq!(runner.monitor().snapshot().phase, SessionPhase::Prepared);
    Ok((runner, client, stop))
}
struct Run {
    worker: Option<thread::JoinHandle<()>>,
    result: mpsc::Receiver<SessionOutcome>,
    stop: UnixDatagram,
}
impl Run {
    fn start(runner: fuser::SessionRunner<Probe>, stop: UnixDatagram) -> Self {
        let (sender, result) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let _ = sender.send(runner.run());
        });
        Self {
            worker: Some(worker),
            result,
            stop,
        }
    }
    fn finish(mut self) -> SessionOutcome {
        let outcome = self
            .result
            .recv_timeout(WAIT)
            .expect("original runner did not finish");
        self.join();
        outcome
    }
    fn join(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        let until = Instant::now() + WAIT;
        while !worker.is_finished() {
            assert!(
                Instant::now() < until,
                "runner failed to exit after owned endpoint shutdown"
            );
            thread::yield_now();
        }
        worker.join().expect("session owner panic");
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.stop.shutdown(Shutdown::Both);
        self.join();
    }
}
fn serving(monitor: &SessionMonitor) {
    let s = monitor.wait_ready(WAIT);
    assert_eq!(s.phase, SessionPhase::Serving, "{s:?}");
    assert_eq!(
        (s.configured, s.created, s.entered, s.exited, s.joined),
        (2, 2, 2, 0, 0)
    );
}
fn joined(monitor: &SessionMonitor, created: usize) {
    let s = monitor.snapshot();
    assert_eq!(s.phase, SessionPhase::Joined);
    assert_eq!((s.created, s.exited, s.joined), (created, created, created));
}
fn synthetic(case: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut fs = Probe::new();
    fs.panic_getattr = case == "panic";
    fs.panic_destroy = case == "destroy-panic";
    let (runner, client, stop) = prepared(fs.clone(), if case == "invalid" { 0 } else { 2 })?;
    let monitor = runner.monitor();
    if case == "partial-spawn" {
        // This process is run under cgroup pids.max=2: caller + one receiver.
        // The first receiver cannot exit before spawn2 because of the rendezvous.
        let outcome = runner.run();
        assert_eq!(
            outcome
                .startup_error
                .as_ref()
                .and_then(io::Error::raw_os_error),
            Some(11)
        );
        assert_eq!(outcome.receivers.len(), 1);
        joined(&monitor, 1);
        assert!(
            matches!(&outcome.receivers[0].result,Ok(Err(error)) if error.kind()==io::ErrorKind::Interrupted)
        );
        assert!(!outcome.is_clean());
    } else if case == "invalid" {
        let outcome = runner.run();
        assert!(outcome.startup_error.is_some());
        assert!(outcome.receivers.is_empty());
        joined(&monitor, 0);
    } else {
        let run = Run::start(runner, stop);
        serving(&monitor);
        assert_eq!(
            fs.callbacks.load(Ordering::SeqCst),
            0,
            "Ready must need no ordinary request"
        );
        if case == "panic" {
            client.send(&frame(3, 2, 16))?;
            let initial = monitor.snapshot();
            let s = if initial.phase == SessionPhase::Serving {
                monitor.wait_for_change(initial.revision, WAIT)
            } else {
                initial
            };
            assert_eq!(s.phase, SessionPhase::Stopping);
            assert_ne!(s.joined, 2, "sibling must still be owned");
            run.stop.shutdown(Shutdown::Both)?;
        } else if case == "io-error" {
            run.stop.shutdown(Shutdown::Both)?;
        } else {
            client.send(&frame(38, 2, 0))?;
            reply(&client, 2)?;
            client.send(&frame(38, 3, 0))?;
            reply(&client, 3)?;
        }
        let outcome = run.finish();
        joined(&monitor, 2);
        assert_eq!(outcome.receivers.len(), 2);
        assert!(outcome.startup_error.is_none());
        if case == "panic" {
            let payload = outcome
                .receivers
                .iter()
                .find_map(|r| r.result.as_ref().err())
                .expect("panic result retained");
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"original receiver panic")
            );
            assert!(!outcome.is_clean());
        } else if case == "destroy-panic" {
            assert!(outcome
                .receivers
                .iter()
                .all(|r| matches!(r.result, Ok(Ok(())))));
            assert_eq!(
                outcome.cleanup.as_ref().unwrap_err().downcast_ref::<&str>(),
                Some(&"original destroy panic")
            );
        } else if case == "io-error" {
            assert!(outcome
                .receivers
                .iter()
                .all(|r| matches!(&r.result,Ok(Err(e)) if e.kind()==io::ErrorKind::InvalidData)));
        } else {
            assert!(outcome.is_clean(), "{outcome:?}");
        }
        // Observing or waiting after disposal retains the Joined observation;
        // the consuming API provides no second run/join that could erase errors.
        assert_eq!(monitor.wait_ready(Duration::ZERO), monitor.snapshot());
    }
    assert_eq!(fs.destroyed.load(Ordering::SeqCst), 1);
    println!(
        "SESSION_LIFECYCLE case={case} PASS snapshot={:?}",
        monitor.snapshot()
    );
    Ok(())
}
fn native(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use nix::mount::{mount, umount2, MntFlags, MsFlags};
    std::fs::create_dir(path)?;
    let device: File = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/fuse")?;
    let session_fd = device.try_clone()?;
    let options = format!(
        "fd={},rootmode=40000,user_id=0,group_id=0,allow_other,default_permissions,max_read=131072",
        device.as_raw_fd()
    );
    mount(
        Some("layerfs-lifecycle-proof"),
        path,
        Some("fuse"),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOATIME,
        Some(options.as_str()),
    )?;
    let fs = Probe::new();
    let mut config = Config::default();
    config.n_threads = Some(2);
    config.clone_fd = false;
    let runner = Session::from_fd(
        fs.clone(),
        OwnedFd::from(session_fd),
        SessionACL::All,
        config,
    )?
    .into_runner();
    let monitor = runner.monitor();
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let _ = sender.send(runner.run());
    });
    serving(&monitor);
    assert_eq!(fs.callbacks.load(Ordering::SeqCst), 0);
    assert!(std::fs::metadata(path)?.is_dir());
    umount2(path, MntFlags::empty())?;
    let outcome = receiver.recv_timeout(WAIT)?;
    let until = Instant::now() + WAIT;
    while !worker.is_finished() {
        assert!(Instant::now() < until);
        thread::yield_now();
    }
    worker.join().map_err(|_| "session owner panic")?;
    assert!(outcome.is_clean(), "{outcome:?}");
    joined(&monitor, 2);
    std::fs::remove_dir(path)?;
    println!(
        "SESSION_LIFECYCLE native PASS snapshot={:?}; dependency scope only",
        monitor.snapshot()
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let case = std::env::args().nth(1).ok_or("case required")?;
    if case == "native" {
        native(Path::new("/tmp/layerfs-session-lifecycle"))
    } else if [
        "ready",
        "panic",
        "io-error",
        "destroy-panic",
        "partial-spawn",
        "invalid",
    ]
    .contains(&case.as_str())
    {
        synthetic(&case)
    } else {
        Err("unknown case".into())
    }
}
