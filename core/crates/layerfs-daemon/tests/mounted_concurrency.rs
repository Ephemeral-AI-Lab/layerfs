//! Real kernel mounts under concurrent external processes: R6-1 (mounted
//! part), R6-2, R6-3 and R6-6, and readers on two Workspaces at once.
//!
//! Every process is python3 launched by plain exec under the command
//! identity and registered with nothing. It reads and writes through ordinary
//! system calls and ends when its standard input is closed or its finite
//! sequence is done. Commits are the product control Commit, attempted once.
//!
//! Evidence is functional results and maintained counts: the owner's public
//! diagnostics, each mount's request counters through control Status, and
//! each process's own exit status and report. Nothing is timed. Two kinds of
//! bound exist and neither is a measurement: every observation loop ends at a
//! wall deadline so that a stalled product fails the test, and a Commit that
//! has not returned after `COMMIT_WAIT` is reported as a stall with the
//! counters of that moment.
//!
//! Limits.
//! - R6-1 here is "no stall observed": 24 processes keep the mount's sixteen
//!   handoff slots contended across three Commits and all of it returns. The
//!   exact slot cycle is not staged here; it is proven at owner scope in
//!   `install_slots.rs`.
//! - Counts taken "before the call" and "after the return" of a Commit bracket
//!   it from outside. Counts labelled "inside" come from a second thread whose
//!   owner snapshot lies between two Status replies that both said
//!   `Committing` in one control epoch.
//! - The stage of a Commit is read from the engine's state row by a
//!   read-only owner job of the test that waits, bounded, for a Lifecycle
//!   slot: no capture yet, a capture retained, or the base root already
//!   replaced. Construction, Save, publication and the wait for install are
//!   not told apart. A stage's count spans its first to its last snapshot.
//! - R6-6 reads its row as totals: 400 writes of 128 KiB over eight processes
//!   on A and 200 small operations over two processes on B.
//! - The two-Workspace readers test checks bytes and completion only. It
//!   counts no owner job and no Store reader; those are pinned per request in
//!   `read_cost.rs`. No Commit runs in it: READ across an install is R6-1.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
use layerfs_bridge::control::{
    Activity, NativeWork, ReadyMount, Reply, Request, WorkspaceStatus, WorkspaceToken,
};
use layerfs_daemon::{control::Service, Command, OwnerClient, OwnerWork, Response, ServiceClass};
use layerfs_history::{CommitRecord, CommitStagedOutcome};
use layerfs_overlay::Route;
use model::{assert_same, Flat, DIRECTORY, FILE};
use mounted::{mount_entry, COMMAND};
use nix::libc;
use rig::{bash_in, native, passed, pattern, receipt, root, Rig};
use std::{
    fmt::Debug,
    fs::{self, OpenOptions},
    future::Future,
    io::{Read, Write},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::Path,
    pin::Pin,
    process::{Child, Command as Process, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc, Mutex,
    },
    task::{Context, Poll, Wake, Waker},
    thread::{self, JoinHandle, Thread},
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the test.
const WAIT: Duration = Duration::from_secs(10);
/// The bounded wait for one Commit to return.
const COMMIT_WAIT: Duration = Duration::from_secs(30);
const CLASSES: [&str; 6] = [
    "read",
    "mutation",
    "capture",
    "lifecycle",
    "record",
    "source",
];

/// Numbered files of one writer until standard input is closed or `cap` files
/// are made. Each number is written to a pending name and renamed into its
/// directory, so one rename publishes one complete numbered file. Reports the
/// last number it renamed.
const WRITER: &str = r#"
import os, select, sys
root, tag, cap = sys.argv[1], sys.argv[2], int(sys.argv[3])
os.umask(0o022)
directory = os.path.join(root, "seq", tag)
pending = os.path.join(root, "work", tag + ".tmp")
done = 0
stopped = False
while done < cap and not stopped:
    number = done + 1
    fd = os.open(pending, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    try:
        os.write(fd, ("writer %s file %06d\n" % (tag, number)).encode())
    finally:
        os.close(fd)
    os.rename(pending, os.path.join(directory, "%06d" % number))
    done = number
    stopped = bool(select.select([sys.stdin], [], [], 0)[0])
if not stopped:
    sys.stdin.read()
sys.stdout.write("%d\n" % done)
"#;
/// Reads base files with `O_DIRECT` (OPEN, READ and RELEASE served by the
/// daemon every time) and creates one numbered file of its own after its
/// first read and then once in `every` reads, at its own `phase`, until
/// standard input is closed. A read that changes or has the wrong length
/// ends it with status 7. Reports its reads and writes.
const READ_WRITE: &str = r#"
import os, select, sys
root, tag, every, phase = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
bases = [entry.split("=") for entry in sys.argv[5:]]
os.umask(0o022)
directory = os.path.join(root, "r61", tag)
first = {}
reads = writes = turn = 0
while not select.select([sys.stdin], [], [], 0)[0]:
    name, length = bases[turn % len(bases)]
    turn += 1
    fd = os.open(os.path.join(root, name), os.O_RDONLY | os.O_DIRECT)
    try:
        data = b""
        while True:
            part = os.read(fd, 65536)
            if not part:
                break
            data += part
    finally:
        os.close(fd)
    if len(data) != int(length) or first.setdefault(name, data) != data:
        sys.exit(7)
    reads += 1
    if turn == 1 or (turn + phase) % every == 0:
        number = writes + 1
        fd = os.open(os.path.join(directory, "%06d" % number), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        try:
            os.write(fd, ("process %s file %06d\n" % (tag, number)).encode())
        finally:
            os.close(fd)
        writes = number
sys.stdout.write("%d %d\n" % (reads, writes))
"#;
/// After one line on standard input: `writes` single write(2) calls of `size`
/// bytes appended to one new file. Block i of process p is the two bytes
/// (p + 1, i) repeated.
const BIG: &str = r#"
import os, sys
root, p, writes, size = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
os.umask(0o022)
sys.stdin.readline()
fd = os.open(os.path.join(root, "big", "p%d" % p), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
for i in range(writes):
    if os.write(fd, bytes([p + 1, i & 255]) * (size // 2)) != size:
        sys.exit(8)
os.close(fd)
sys.stdout.write("%d\n" % writes)
"#;
/// After one line on standard input: `count` small operations, each one
/// system call sequence on one name, in groups of five: create with content,
/// append, rename, mkdir, chmod.
const SMALL: &str = r#"
import os, sys
root, p, count = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
os.umask(0o022)
sys.stdin.readline()
directory = os.path.join(root, "small", "p%d" % p)
done = 0
for i in range(count):
    group = i // 5 * 5
    name = os.path.join(directory, "f-%03d" % group)
    moved = os.path.join(directory, "g-%03d" % group)
    kind = i % 5
    if kind == 0:
        fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        os.write(fd, ("small %d %03d\n" % (p, group)).encode())
        os.close(fd)
    elif kind == 1:
        fd = os.open(name, os.O_WRONLY | os.O_APPEND)
        os.write(fd, b"+\n")
        os.close(fd)
    elif kind == 2:
        os.rename(name, moved)
    elif kind == 3:
        os.mkdir(os.path.join(directory, "d-%03d" % group), 0o755)
    else:
        os.chmod(moved, 0o600)
    done += 1
sys.stdout.write("%d\n" % done)
"#;
/// After one line on standard input: `turns` whole reads with `O_DIRECT`
/// (OPEN, every READ of 16 KiB and RELEASE served by the daemon), one file per
/// turn starting at file `start`. Each file is `name=seed=length=offset=text`:
/// the rig's pattern with `text` written at `offset`. A read that is not exactly
/// that ends the process with status 7. Reports its turns.
const READER: &str = r#"
import os, sys
root, turns, start = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
files = []
for entry in sys.argv[4:]:
    name, seed, length, offset, patch = entry.split("=")
    want = bytearray((n * 31 + (n >> 9) + int(seed)) & 255 for n in range(int(length)))
    patch = patch.encode()
    want[int(offset):int(offset) + len(patch)] = patch
    files.append((name, bytes(want)))
sys.stdin.readline()
done = 0
for turn in range(turns):
    name, want = files[(start + turn) % len(files)]
    fd = os.open(os.path.join(root, name), os.O_RDONLY | os.O_DIRECT)
    try:
        data = b""
        while True:
            part = os.read(fd, 16384)
            if not part:
                break
            data += part
    finally:
        os.close(fd)
    if data != want:
        sys.exit(7)
    done += 1
sys.stdout.write("%d\n" % done)
"#;

fn within(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(600);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "CONCURRENCY_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// One external python3 process of the command identity. Killed and reaped,
/// bounded, on drop.
struct Proc {
    child: Option<Child>,
    what: String,
}
impl Proc {
    fn python(what: impl Into<String>, script: &str, arguments: &[String]) -> Self {
        let child = Process::new("python3")
            .arg("-c")
            .arg(script)
            .args(arguments)
            .uid(COMMAND)
            .gid(COMMAND)
            .current_dir("/")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            child: Some(child),
            what: what.into(),
        }
    }
    /// The one line a gated script waits for.
    fn go(&mut self) {
        let child = self.child.as_mut().unwrap();
        child.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
    }
    fn running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| child.try_wait().unwrap().is_none())
    }
    /// Closes its standard input, waits, bounded, for it to exit 0 and
    /// returns the numbers of its report.
    fn release(&mut self) -> Vec<usize> {
        let mut child = self.child.take().expect("process already collected");
        drop(child.stdin.take());
        let deadline = Instant::now() + WAIT;
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                // Back under the guard, which kills and reaps it.
                self.child = Some(child);
                panic!("{}: still running {WAIT:?} after its stop", self.what);
            }
            thread::sleep(Duration::from_millis(2));
        };
        let mut report = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut report)
            .unwrap();
        assert!(status.success(), "{}: {status:?} {report:?}", self.what);
        report
            .split_whitespace()
            .map(|number| number.parse().unwrap())
            .collect()
    }
}
/// How many of the processes have not exited.
fn running(processes: &mut [Proc]) -> usize {
    let mut count = 0;
    for process in processes {
        count += usize::from(process.running());
    }
    count
}
impl Drop for Proc {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let deadline = Instant::now() + Duration::from_secs(3);
        while matches!(child.try_wait(), Ok(None)) {
            if Instant::now() >= deadline {
                println!(
                    "CONCURRENCY_GUARD {} (pid {}) was killed and is not reaped",
                    self.what,
                    child.id()
                );
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
/// A second thread calling `sample` until it is finished or dropped, and for
/// at most a minute.
struct Sampler {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Sampler {
    fn start(every: Duration, mut sample: impl FnMut() + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
                sample();
                thread::sleep(every);
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
    fn finish(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let thread = self.thread.take().unwrap();
        within("the sampling thread ended", || thread.is_finished());
        thread.join().unwrap();
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn status(service: &Service, token: WorkspaceToken) -> Option<WorkspaceStatus> {
    match service.execute_control(&Request::Status(token)).ok()?.reply {
        Reply::Status(status) => Some(*status),
        _ => None,
    }
}
struct Unpark(Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
/// One read-only engine observation of a Workspace through the owner's
/// public admission wait: whether a capture is retained, and the installed
/// base root. Control Status asks for the same row without waiting and is
/// refused whenever both Lifecycle slots of the lane are in use. `None` when
/// no slot was free, or the job was not done, within `wait`; dropping the
/// unadmitted wait cancels only that wait.
fn engine(owner: &OwnerClient, route: Route, wait: Duration) -> Option<(bool, [u8; 32])> {
    let deadline = Instant::now() + wait;
    let mut admission = owner
        .submit_when_available(Some(route), Command::State)
        .ok()?;
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let pending = loop {
        match Pin::new(&mut admission).poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(outcome) => break outcome.ok()?,
            Poll::Pending => {
                let now = Instant::now();
                if now >= deadline {
                    return None;
                }
                thread::park_timeout(deadline - now);
            }
        }
    };
    loop {
        match pending.try_complete() {
            Ok(Some(done)) => {
                return match done.result() {
                    Ok(Response::State(state)) => Some((state.captured.is_some(), state.base_root)),
                    _ => None,
                }
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_micros(100)),
            _ => return None,
        }
    }
}
fn work(rig: &Rig, token: WorkspaceToken) -> NativeWork {
    rig.harness.status(token).native.unwrap().work.unwrap()
}
fn completed(before: &OwnerWork, after: &OwnerWork) -> [u64; 6] {
    std::array::from_fn(|class| after.completed[class] - before.completed[class])
}
fn classes(delta: [u64; 6]) -> String {
    CLASSES
        .iter()
        .zip(delta)
        .map(|(class, count)| format!("{class}=+{count}"))
        .collect::<Vec<_>>()
        .join(" ")
}
/// Completed owner jobs of the two classes only filesystem requests submit:
/// no step of a Commit is a Mutation or a Source job.
fn filesystem_only(delta: [u64; 6]) -> u64 {
    delta[ServiceClass::Mutation as usize] + delta[ServiceClass::Source as usize]
}
/// Names in one directory of a mount, by `getdents`.
fn names(directory: &Path) -> usize {
    let mut count = 0;
    for entry in fs::read_dir(directory).unwrap() {
        entry.unwrap();
        count += 1;
    }
    count
}
/// The file's bytes as the daemon serves them now.
fn direct(path: &Path) -> Vec<u8> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
        .unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between, then the normal drained unmount.
fn unmount(rig: &Rig, ready: &ReadyMount) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(rig, ready.token);
        let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
    rig.unmount(ready);
}

/// The Commit did not return inside its bounded wait. Prints what the daemon
/// reports at that moment and fails the test; the Commit's thread is left as
/// it is. Nothing is retried or settled.
fn stalled(rig: &Rig, token: WorkspaceToken, what: &str) -> ! {
    println!("COMMIT STALLED {what}: no return within {COMMIT_WAIT:?}");
    println!(
        "COMMIT STALLED owner={}",
        brief(&rig.harness.owner.client().diagnostics().map(|work| (
            work.admitted,
            work.completed,
            work.outstanding,
            work.queued,
            work.peak_queued
        )))
    );
    println!("COMMIT STALLED store_reads={:?}", rig.store.read_work());
    // Status issues one owner job: observed from a thread the test can leave.
    let (sender, receiver) = mpsc::channel();
    let service = rig.harness.service.clone();
    thread::spawn(move || {
        let _ = sender.send(status(&service, token).map(|status| {
            format!(
                "activity={:?} epoch={} local={:?} native={:?}",
                status.activity, status.epoch, status.local, status.native
            )
        }));
    });
    println!(
        "COMMIT STALLED status={:?}",
        receiver.recv_timeout(Duration::from_secs(3))
    );
    panic!("{what}: the Commit did not return within {COMMIT_WAIT:?}");
}
/// One product control Commit on its own thread, attempted once, which must
/// publish a new record with the constructor's complete receipt. A Commit
/// that has not returned after `COMMIT_WAIT` is a stall.
fn commit(rig: &Rig, token: WorkspaceToken, what: &str) -> CommitRecord {
    let service = rig.harness.service.clone();
    let label = what.to_owned();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let outcome = match service.execute_control(&Request::Commit(token)) {
            Ok(done) => {
                receipt(&done, &label);
                match &done.reply {
                    Reply::Committed(CommitStagedOutcome::Committed(record)) => Ok(record.clone()),
                    other => Err(brief(other)),
                }
            }
            Err(failure) => Err(brief(&failure)),
        };
        let _ = sender.send(outcome);
    });
    match receiver.recv_timeout(COMMIT_WAIT) {
        Ok(outcome) => {
            worker.join().unwrap();
            outcome.unwrap_or_else(|refused| {
                panic!("{what}: the Commit was refused or published nothing: {refused}")
            })
        }
        Err(RecvTimeoutError::Disconnected) => {
            std::panic::resume_unwind(worker.join().unwrap_err())
        }
        Err(RecvTimeoutError::Timeout) => stalled(rig, token, what),
    }
}

fn numbered(tag: &str, number: usize) -> Vec<u8> {
    format!("writer {tag} file {number:06}\n").into_bytes()
}
/// The numbered files of one writer in a tree: exactly the files 1 to k,
/// each complete. Returns k.
fn sequence(flat: &Flat, tag: &str, what: &str) -> usize {
    let prefix = format!("seq/{tag}/");
    let directory = flat
        .get(prefix.trim_end_matches('/').as_bytes())
        .unwrap_or_else(|| panic!("{what}: seq/{tag} is absent"));
    assert_eq!(
        (directory.kind, directory.mode),
        (DIRECTORY, 0o755),
        "{what}"
    );
    let mut files = 0;
    for (path, seen) in flat
        .iter()
        .filter(|(path, _)| path.starts_with(prefix.as_bytes()))
    {
        files += 1;
        assert_eq!(
            String::from_utf8_lossy(path),
            format!("{prefix}{files:06}"),
            "{what}: the numbered files of {tag} are not a contiguous prefix at {files}"
        );
        assert_eq!(
            (seen.kind, seen.mode, seen.links),
            (FILE, 0o644, 1),
            "{what}: {prefix}{files:06}"
        );
        assert!(
            seen.payload == numbered(tag, files),
            "{what}: content of {prefix}{files:06}: {:?}",
            String::from_utf8_lossy(&seen.payload)
        );
    }
    files
}
/// The pending name of one writer beside its k numbered files: absent, or
/// created and still empty, or the whole next file. Returns which.
fn pending(flat: &Flat, tag: &str, k: usize, what: &str) -> &'static str {
    match flat.get(format!("work/{tag}.tmp").as_bytes()) {
        None => "absent",
        Some(seen) => {
            assert_eq!(seen.kind, FILE, "{what}: work/{tag}.tmp");
            if seen.payload.is_empty() {
                "empty"
            } else {
                assert!(
                    seen.payload == numbered(tag, k + 1),
                    "{what}: work/{tag}.tmp beside {k} files: {:?}",
                    String::from_utf8_lossy(&seen.payload)
                );
                "complete"
            }
        }
    }
}
/// Everything the writers do not touch.
fn rest(flat: &Flat) -> Flat {
    flat.iter()
        .filter(|(path, _)| !(path.starts_with(b"seq/") || path.starts_with(b"work")))
        .map(|(path, seen)| (path.clone(), seen.clone()))
        .collect()
}

/// What the sampling thread saw of one mount while one Commit ran.
#[derive(Clone, Copy, Default)]
struct Peak {
    samples: u64,
    admitted: u32,
    received: u32,
    parked: u32,
}
/// Base files the R6-1 processes read, with the seed of their bytes.
const READ: [(&str, u8, usize); 4] = [
    ("docs/guide.md", 10, 5_000),
    ("node_modules/pkg/lib/util.js", 4, 12_000),
    (".cache/tool/data.bin", 6, 20_000),
    ("dist/bundle.js", 9, 33_000),
];

/// R6-1, mounted part. No stall observed: 24 external processes read base
/// files and create files on one mount while three Commits run one after the
/// other; every Commit returns and every process exits 0. This does not stage
/// the exact slot cycle.
#[test]
fn r6_1_twenty_four_processes_read_and_write_through_three_commits() {
    const PROCESSES: usize = 24;
    /// One created file for this many reads of a process, each process at
    /// its own phase: files arrive throughout every Commit, and more slowly
    /// than a Commit constructs them, so the three Commits stay comparable.
    const EVERY: usize = 48;
    let rig = Rig::new("r6-1");
    let ready = rig.mount(1);
    let _mounted = Mounted(ready.directory.clone());
    let mount = root(&ready);
    let tag = |process: usize| format!("p{process:02}");
    passed(
        &bash_in(
            COMMAND,
            mount,
            "set -euo pipefail; umask 022; mkdir r61; for p in $(seq -f '%02g' 0 23); do mkdir r61/p$p; done",
        ),
        "prepare",
    );
    let made = || -> usize {
        (0..PROCESSES)
            .map(|process| names(&mount.join("r61").join(tag(process))))
            .sum()
    };
    let current = Arc::new(AtomicUsize::new(0));
    let peaks = Arc::new(Mutex::new([Peak::default(); 4]));
    let sampler = {
        let (service, token) = (rig.harness.service.clone(), ready.token);
        let (current, peaks) = (current.clone(), peaks.clone());
        Sampler::start(Duration::from_millis(2), move || {
            let commit = current.load(Ordering::SeqCst);
            let Some(now) = status(&service, token).and_then(|status| status.native?.work) else {
                return;
            };
            // Counted for a Commit only if it was still the current one
            // after the observation.
            if current.load(Ordering::SeqCst) != commit {
                return;
            }
            let mut peaks = peaks.lock().unwrap();
            let peak = &mut peaks[commit];
            peak.samples += 1;
            peak.admitted = peak.admitted.max(now.admitted);
            peak.received = peak.received.max(now.received);
            peak.parked = peak.parked.max(now.parked);
        })
    };
    let mut processes: Vec<Proc> = (0..PROCESSES)
        .map(|process| {
            let mut arguments = vec![
                mount.to_str().unwrap().to_owned(),
                tag(process),
                EVERY.to_string(),
                (process * (EVERY / PROCESSES)).to_string(),
            ];
            arguments.extend(
                READ.iter()
                    .map(|(path, _, length)| format!("{path}={length}")),
            );
            Proc::python(format!("process {process}"), READ_WRITE, &arguments)
        })
        .collect();
    within("every process created a file", || {
        (0..PROCESSES).all(|process| names(&mount.join("r61").join(tag(process))) >= 1)
    });

    let owner = rig.harness.owner.client();
    let mut parent = rig.harness.status(ready.token).binding.branch.head_commit;
    let mut files = made();
    for number in 1..=3 {
        let what = format!("R6-1 Commit {number} under {PROCESSES} processes");
        let (before, before_work) = (owner.diagnostics().unwrap(), work(&rig, ready.token));
        current.store(number, Ordering::SeqCst);
        let record = commit(&rig, ready.token, &what);
        current.store(0, Ordering::SeqCst);
        let (after, after_work) = (owner.diagnostics().unwrap(), work(&rig, ready.token));
        let alive = running(&mut processes);
        let now = made();
        let peak = peaks.lock().unwrap()[number];
        println!(
            "{what}: returned=true alive={alive} files_before_call={files} files_after_return={now} mount_completed=+{} mount_handoffs=+{} owner_completed {} filesystem_only=+{} samples_while_current={} max_admitted={} max_received={} max_parked={} mount_retained={}",
            after_work.completed - before_work.completed,
            after_work.handoffs - before_work.handoffs,
            classes(completed(&before, &after)),
            filesystem_only(completed(&before, &after)),
            peak.samples,
            peak.admitted,
            peak.received,
            peak.parked,
            after_work.retained
        );
        assert_eq!(record.parent, parent, "{what}: parent is the previous head");
        assert_eq!(alive, PROCESSES, "{what}: every process is still running");
        assert!(
            now > files,
            "{what}: no file was created between its call and its return ({files} then {now})"
        );
        assert_eq!((after_work.retained, after_work.terminal), (0, 0), "{what}");
        parent = Some(record.id);
        files = now;
    }
    sampler.finish();

    let mut reports = Vec::new();
    for process in &mut processes {
        let report = process.release();
        assert!(
            report.len() == 2 && report[0] >= 1 && report[1] >= 1,
            "{}: read and wrote: {report:?}",
            process.what
        );
        reports.push((report[0], report[1]));
    }
    drop(processes);
    // What the processes left: every file each reports, whole, and the base
    // files they read, unchanged.
    let live = native(mount);
    for (process, (_, writes)) in reports.iter().enumerate() {
        let tag = tag(process);
        let made: Vec<_> = live
            .iter()
            .filter(|(path, _)| path.starts_with(format!("r61/{tag}/").as_bytes()))
            .collect();
        assert_eq!(made.len(), *writes, "process {process}");
        for (index, (path, seen)) in made.into_iter().enumerate() {
            let number = index + 1;
            assert_eq!(
                String::from_utf8_lossy(path),
                format!("r61/{tag}/{number:06}")
            );
            assert!(
                seen.payload == format!("process {tag} file {number:06}\n").into_bytes(),
                "process {process} file {number}"
            );
        }
    }
    for (path, seed, length) in READ {
        assert!(
            live[path.as_bytes()].payload == pattern(seed, length),
            "{path} is unchanged"
        );
    }
    let last = work(&rig, ready.token);
    let peak = owner.diagnostics().unwrap();
    println!(
        "R6-1 mounted: no stall observed. processes={PROCESSES} commits=3 exits_zero={} reads={} writes={} mount_completed={} mount_retained={} owner_peak_queued={} owner_outstanding={}",
        reports.len(),
        reports.iter().map(|(reads, _)| reads).sum::<usize>(),
        reports.iter().map(|(_, writes)| writes).sum::<usize>(),
        last.completed,
        last.retained,
        peak.peak_queued,
        peak.outstanding
    );
    unmount(&rig, &ready);
    rig.finish();
}

/// The first and the last owner snapshot of a run of samples.
type Span = Option<(OwnerWork, OwnerWork)>;
/// Owner snapshots that lie inside one Commit, by stage.
#[derive(Default)]
struct Inside {
    samples: u64,
    span: Span,
    /// No capture yet; a capture retained; the base root already replaced.
    stages: [(u64, Span); 3],
    /// Samples with no engine observation inside the test's bounded wait.
    unobserved: u64,
    /// Samples in which control Status's own engine observation was refused.
    status_unobserved: u64,
}
impl Inside {
    fn note(span: &mut Span, now: OwnerWork) {
        match span {
            Some((_, last)) => *last = now,
            None => *span = Some((now, now)),
        }
    }
    fn delta(span: &Span) -> [u64; 6] {
        span.as_ref()
            .map_or([0; 6], |(first, last)| completed(first, last))
    }
}

/// R6-2: one Commit against twenty external writers that never pause, on one
/// Workspace. The Commit is not refused; its root holds an exact prefix of
/// every writer's sequence; the mount shows everything made; the next Commit
/// holds the rest; a fresh mount is the oracle. The owner jobs admitted and
/// completed while the Commit was in progress are recorded by count.
///
/// Two limits of that record, as observed in
/// `P1-attempt2-linux-mounted_concurrency` and
/// `P1-attempt3-linux-mounted_concurrency`:
/// - The counts are attributed to one Commit stage only. Every staged sample
///   (278 of 279, then 326 of 327) fell in "capture retained"; "no capture
///   yet" and "base root replaced" had no sample, so nothing is recorded for
///   the jobs admitted before the capture or after the install.
/// - Control Status's own engine observation (`local`) was refused in most
///   samples under the twenty writers (268 of 279, then 318 of 327): it does
///   not wait for a Lifecycle slot. The stage therefore comes from the test's
///   own waiting, bounded, read-only `State` job, not from Status.
#[test]
fn r6_2_a_commit_against_twenty_writers_holds_an_exact_prefix_of_each() {
    const WRITERS: usize = 20;
    /// A writer's whole sequence. One that reaches it stops writing, so a
    /// Commit that outlasts it was not under twenty writers to its end.
    const CAP: usize = 400;
    let what = "R6-2";
    let rig = Rig::new("r6-2");
    let ready = rig.mount(1);
    let _mounted = Mounted(ready.directory.clone());
    let mount = root(&ready);
    let tag = |writer: usize| format!("w{writer:02}");
    passed(
        &bash_in(
            COMMAND,
            mount,
            "set -euo pipefail; umask 022; mkdir seq work; for w in $(seq -f '%02g' 0 19); do mkdir seq/w$w; done",
        ),
        "prepare",
    );
    let prepared = native(mount);
    let made = || -> Vec<usize> {
        (0..WRITERS)
            .map(|writer| names(&mount.join("seq").join(tag(writer))))
            .collect()
    };
    let bound = rig.harness.status(ready.token);
    let owner = rig.harness.owner.client();
    let route = rig
        .harness
        .service
        .operation(ready.token)
        .unwrap()
        .workspace()
        .route();
    let (_, base_root) = engine(&owner, route, WAIT).expect("engine observation");

    let armed = Arc::new(AtomicBool::new(false));
    let inside = Arc::new(Mutex::new(Inside::default()));
    let sampler = {
        let (service, token, owner) = (rig.harness.service.clone(), ready.token, owner.clone());
        let (armed, inside) = (armed.clone(), inside.clone());
        Sampler::start(Duration::from_millis(5), move || {
            if !armed.load(Ordering::SeqCst) {
                return;
            }
            let committing = |service: &Service| {
                status(service, token).filter(|status| status.activity == Activity::Committing)
            };
            // No capture yet; a capture retained; the base root replaced.
            let stage = || {
                engine(&owner, route, Duration::from_millis(20)).map(|(captured, root)| {
                    if captured {
                        1
                    } else if root != base_root {
                        2
                    } else {
                        0
                    }
                })
            };
            let Some(first) = committing(&service) else {
                return;
            };
            let before = stage();
            let Ok(now) = owner.diagnostics() else {
                return;
            };
            let after = stage();
            let Some(second) = committing(&service) else {
                return;
            };
            if first.epoch != second.epoch {
                return;
            }
            let mut inside = inside.lock().unwrap();
            inside.samples += 1;
            Inside::note(&mut inside.span, now);
            inside.status_unobserved += u64::from(first.local.is_none());
            // The stage is taken only when the engine said the same before
            // and after the snapshot.
            match (before, after) {
                (Some(before), Some(after)) if before == after => {
                    inside.stages[before].0 += 1;
                    Inside::note(&mut inside.stages[before].1, now);
                }
                (None, _) | (_, None) => inside.unobserved += 1,
                _ => {}
            }
        })
    };

    let mut writers: Vec<Proc> = (0..WRITERS)
        .map(|writer| {
            Proc::python(
                format!("writer {writer}"),
                WRITER,
                &[
                    mount.to_str().unwrap().to_owned(),
                    tag(writer),
                    CAP.to_string(),
                ],
            )
        })
        .collect();
    within("every writer renamed three files into place", || {
        made().iter().all(|count| *count >= 3)
    });

    // One Commit while all twenty write.
    let before_call = made();
    let (before, before_work) = (owner.diagnostics().unwrap(), work(&rig, ready.token));
    armed.store(true, Ordering::SeqCst);
    let record = commit(&rig, ready.token, "R6-2 under twenty writers");
    let (after, after_work) = (owner.diagnostics().unwrap(), work(&rig, ready.token));
    armed.store(false, Ordering::SeqCst);
    let at_return = made();
    let alive = running(&mut writers);
    sampler.finish();
    assert_eq!(record.parent, bound.binding.branch.head_commit);

    let across = completed(&before, &after);
    println!(
        "{what} owner jobs, before the call -> after the return: admitted=+{} completed {} filesystem_only(mutation+source)=+{} mount_completed=+{} mount_handoffs=+{} outstanding={}->{} peak_queued={}",
        after.admitted - before.admitted,
        classes(across),
        filesystem_only(across),
        after_work.completed - before_work.completed,
        after_work.handoffs - before_work.handoffs,
        before.outstanding,
        after.outstanding,
        after.peak_queued
    );
    let (within_commit, admitted_inside) = {
        let inside = inside.lock().unwrap();
        let span = Inside::delta(&inside.span);
        let admitted = inside
            .span
            .as_ref()
            .map_or(0, |(first, last)| last.admitted - first.admitted);
        println!(
            "{what} owner jobs inside the Commit (Status said Committing before and after each snapshot): samples={} admitted=+{admitted} completed {} filesystem_only=+{} stage_unobserved={} status_engine_observation_refused={}",
            inside.samples,
            classes(span),
            filesystem_only(span),
            inside.unobserved,
            inside.status_unobserved
        );
        for (stage, (samples, span)) in ["no capture yet", "capture retained", "base root replaced"]
            .iter()
            .zip(&inside.stages)
        {
            let delta = Inside::delta(span);
            println!(
                "{what} stage \"{stage}\": samples={samples} completed {} filesystem_only=+{}",
                classes(delta),
                filesystem_only(delta)
            );
        }
        (span, admitted)
    };
    println!(
        "{what} writers: alive_at_return={alive} files_before_call={} files_at_return={} exhausted_at_return={}",
        before_call.iter().sum::<usize>(),
        at_return.iter().sum::<usize>(),
        at_return.iter().filter(|count| **count >= CAP).count()
    );
    assert_eq!(alive, WRITERS, "every writer outlived the Commit");
    assert!(
        at_return.iter().all(|count| *count < CAP),
        "{what}: the Commit outlasted a writer's whole sequence of {CAP} files, so it was not under twenty writers to its end: {at_return:?}"
    );
    // Filesystem requests were served while the Commit was in progress.
    assert!(
        filesystem_only(within_commit) > 0 && admitted_inside > 0,
        "{what}: no filesystem owner job was seen inside the Commit"
    );
    assert_eq!((after_work.retained, after_work.terminal), (0, 0));

    let reported: Vec<usize> = writers
        .iter_mut()
        .map(|writer| {
            let report = writer.release();
            assert_eq!(report.len(), 1, "{}: {report:?}", writer.what);
            report[0]
        })
        .collect();
    drop(writers);

    // The committed root: an exact prefix of each writer's sequence.
    let first = rig.published(record.root);
    let live = native(mount);
    let mut captured = Vec::new();
    let mut pendings = [0usize; 3];
    for writer in 0..WRITERS {
        let tag = tag(writer);
        let k = sequence(&first, &tag, "the committed root");
        let state = pending(&first, &tag, k, "the committed root");
        pendings[["absent", "empty", "complete"]
            .iter()
            .position(|name| *name == state)
            .unwrap()] += 1;
        // The mount shows everything the writer made, and nothing pending.
        let n = sequence(&live, &tag, "the mount");
        assert_eq!(
            n, reported[writer],
            "the mount shows all of writer {writer}"
        );
        assert_eq!(pending(&live, &tag, n, "the mount"), "absent");
        assert!(
            before_call[writer] <= k && k <= at_return[writer] && at_return[writer] <= n,
            "writer {writer}: {} before the call, {k} captured, {} at the return, {n} made",
            before_call[writer],
            at_return[writer]
        );
        // A numbered file never changes after its rename.
        for number in 1..=k {
            let path = format!("seq/{tag}/{number:06}").into_bytes();
            assert_eq!(first[&path], live[&path], "writer {writer} file {number}");
        }
        captured.push(k);
    }
    assert_same(
        &rest(&prepared),
        &rest(&first),
        "the committed root, the rest",
    );
    assert_same(&rest(&prepared), &rest(&live), "the mount, the rest");
    let (k, at, n) = (
        captured.iter().sum::<usize>(),
        at_return.iter().sum::<usize>(),
        reported.iter().sum::<usize>(),
    );
    println!(
        "{what} prefix: writers={WRITERS} captured={k} per_writer_min={} per_writer_max={} pending_absent={} pending_empty={} pending_complete={} at_return={at} made={n} published_after_the_capture_and_before_the_return>={}",
        captured.iter().min().unwrap(),
        captured.iter().max().unwrap(),
        pendings[0],
        pendings[1],
        pendings[2],
        at - k
    );
    assert!(
        at > k,
        "{what}: no numbered file followed the capture before the return ({k} captured, {at} at the return)"
    );

    // The next Commit holds the rest.
    let second = commit(&rig, ready.token, "R6-2 the rest");
    assert_eq!(second.parent, Some(record.id));
    let all = rig.published(second.root);
    assert_same(&live, &all, "the next Commit's root");
    assert_same(&live, &native(mount), "the same mount after both installs");
    unmount(&rig, &ready);

    // Fresh-mount oracle.
    let fresh = rig.mount(2);
    let _fresh = Mounted(fresh.directory.clone());
    assert_eq!(
        rig.harness.status(fresh.token).binding.effective_root,
        second.root
    );
    assert_same(&live, &native(root(&fresh)), "the fresh mount");
    println!(
        "{what}: commit_refused=false prefix_exact=true mount_shows_all={n} next_commit_holds_rest={} fresh_mount_equal=true paths={}",
        n - k,
        live.len()
    );
    unmount(&rig, &fresh);
    rig.finish();
}

/// B's completed requests seen while A's Commit ran.
#[derive(Default)]
struct During {
    samples: u64,
    first: Option<u64>,
    last: u64,
}

/// R6-3: two Workspaces mounted on one daemon. B's writer runs through A's
/// Commit. A's root has none of B's names; B completes requests between the
/// Commit's start and its return; B's later Commit is exact.
#[test]
fn r6_3_a_commit_on_one_workspace_while_its_sibling_writes() {
    /// B's whole sequence; it must outlast A's Commit.
    const CAP: usize = 4_000;
    /// Changed files in A, so that its Commit spans many of B's requests.
    const BULK: usize = 150;
    let what = "R6-3";
    let rig = Rig::new("r6-3");
    let a = rig.mount(1);
    let b = rig.mount(2);
    let _mounted = [Mounted(a.directory.clone()), Mounted(b.directory.clone())];
    let (mount_a, mount_b) = (root(&a), root(&b));
    let original = rig.harness.status(a.token).binding.branch.head_commit;
    passed(
        &bash_in(
            COMMAND,
            mount_a,
            &format!(
                "set -euo pipefail; umask 022; mkdir -p bulk; i=0; while [ \"$i\" -lt {BULK} ]; do i=$((i+1)); printf 'only in A %04d\\n' \"$i\" > bulk/f-$i; done; printf 'A changed the readme\\n' >> README.md"
            ),
        ),
        "changes in A",
    );
    passed(
        &bash_in(
            COMMAND,
            mount_b,
            "set -euo pipefail; umask 022; mkdir -p seq/b work",
        ),
        "directories in B",
    );
    let tree_a = native(mount_a);
    let prepared_b = native(mount_b);
    let made = || names(&mount_b.join("seq/b"));

    let armed = Arc::new(AtomicBool::new(false));
    let during = Arc::new(Mutex::new(During::default()));
    let sampler = {
        let service = rig.harness.service.clone();
        let (token_a, token_b) = (a.token, b.token);
        let (armed, during) = (armed.clone(), during.clone());
        Sampler::start(Duration::from_millis(2), move || {
            if !armed.load(Ordering::SeqCst) {
                return;
            }
            // B's count is taken between two replies that both say A is
            // Committing in one control epoch: inside A's Commit.
            let committing = |service: &Service| {
                status(service, token_a).filter(|status| status.activity == Activity::Committing)
            };
            let Some(first) = committing(&service) else {
                return;
            };
            let Some(now) = status(&service, token_b).and_then(|status| status.native?.work) else {
                return;
            };
            let Some(second) = committing(&service) else {
                return;
            };
            if first.epoch != second.epoch {
                return;
            }
            let mut during = during.lock().unwrap();
            during.samples += 1;
            if during.first.is_none() {
                during.first = Some(now.completed);
            }
            during.last = now.completed;
        })
    };
    let mut writer = Proc::python(
        "B's writer",
        WRITER,
        &[
            mount_b.to_str().unwrap().to_owned(),
            "b".to_owned(),
            CAP.to_string(),
        ],
    );
    within("B's writer renamed five files into place", || made() >= 5);

    let (b_call, made_call) = (work(&rig, b.token), made());
    armed.store(true, Ordering::SeqCst);
    let record_a = commit(&rig, a.token, "R6-3 A while B writes");
    let (b_return, made_return) = (work(&rig, b.token), made());
    armed.store(false, Ordering::SeqCst);
    let alive = writer.running();
    sampler.finish();
    let (samples, first, last) = {
        let during = during.lock().unwrap();
        (during.samples, during.first.unwrap_or(0), during.last)
    };
    println!(
        "{what} B while A committed: b_completed before_call={} after_return={} inside_first={first} inside_last={last} inside_samples={samples} b_files before_call={made_call} after_return={made_return} writer_alive={alive}",
        b_call.completed, b_return.completed
    );
    assert_eq!(record_a.parent, original);
    assert!(
        alive && made_return < CAP,
        "B's writer ran through A's Commit"
    );
    assert!(
        b_return.completed > b_call.completed,
        "{what}: B completed nothing across A's Commit"
    );
    assert!(
        samples >= 2 && last > first,
        "{what}: B's completed count did not advance inside A's Commit: {first} -> {last} in {samples} samples"
    );
    assert_eq!((b_return.retained, b_return.terminal), (0, 0));
    let made_b = writer.release();
    assert_eq!(made_b.len(), 1);
    let n = made_b[0];
    drop(writer);

    // A's root has none of B's names: it is exactly A's own view.
    let root_a = rig.published(record_a.root);
    assert_same(&tree_a, &root_a, "A's published root");
    assert!(
        root_a
            .keys()
            .all(|path| !(path.starts_with(b"seq") || path.starts_with(b"work"))),
        "{what}: one of B's names is in A's root"
    );
    assert_eq!(
        root_a
            .keys()
            .filter(|path| path.starts_with(b"bulk/"))
            .count(),
        BULK
    );
    assert_same(&tree_a, &native(mount_a), "A's mount after its install");
    // B is still bound at the earlier head and has none of A's names.
    let stale = rig.harness.status(b.token).binding;
    assert_eq!(stale.branch.head_commit, original);
    let tree_b = native(mount_b);
    assert!(
        tree_b.keys().all(|path| !path.starts_with(b"bulk")),
        "{what}: one of A's names is in B's view"
    );
    assert_eq!(sequence(&tree_b, "b", "B's mount"), n);
    assert_eq!(pending(&tree_b, "b", n, "B's mount"), "absent");
    assert_same(&rest(&prepared_b), &rest(&tree_b), "B's mount, the rest");

    // B's later Commit is exact: its own view, on its captured parent.
    let record_b = commit(&rig, b.token, "R6-3 B's later Commit");
    assert_eq!(record_b.parent, original, "the captured parent");
    assert_ne!(record_b.root, record_a.root);
    let root_b = rig.published(record_b.root);
    assert_same(&tree_b, &root_b, "B's published root");
    assert_same(&tree_b, &native(mount_b), "B's mount after its install");
    assert_same(&tree_a, &native(mount_a), "A's mount after B's Commit");
    println!(
        "{what}: a_root_paths={} b_names_in_a_root=0 a_names_in_b_view=0 b_made={n} b_root_paths={} b_later_commit_exact=true",
        root_a.len(),
        root_b.len()
    );
    unmount(&rig, &a);
    unmount(&rig, &b);
    rig.finish();
}

/// Consecutive samples in which both mounts completed requests.
#[derive(Default)]
struct Together {
    samples: u64,
    last: Option<(u64, u64)>,
    both_advanced: u64,
}

/// R6-6: finite arrivals on two Workspaces at once. A: eight processes, 400
/// writes of 128 KiB in all. B: two processes, 200 small operations in all,
/// and one Commit while they arrive. Both finish; each mount completed at
/// least one request for every write or operation; every owner service class
/// advanced; nothing is retained. Counts only: no share and no time.
#[test]
fn r6_6_finite_arrivals_on_two_workspaces_both_finish() {
    const BIG_PROCESSES: usize = 8;
    const WRITES_EACH: usize = 50;
    const BLOCK: usize = 128 * 1024;
    const SMALL_PROCESSES: usize = 2;
    const OPERATIONS_EACH: usize = 100;
    let what = "R6-6";
    let rig = Rig::new("r6-6");
    let a = rig.mount(1);
    let b = rig.mount(2);
    let _mounted = [Mounted(a.directory.clone()), Mounted(b.directory.clone())];
    let (mount_a, mount_b) = (root(&a), root(&b));
    passed(
        &bash_in(COMMAND, mount_a, "set -euo pipefail; umask 022; mkdir big"),
        "directory in A",
    );
    passed(
        &bash_in(
            COMMAND,
            mount_b,
            "set -euo pipefail; umask 022; mkdir -p small/p0 small/p1",
        ),
        "directories in B",
    );
    let prepared_b = native(mount_b);
    let owner = rig.harness.owner.client();
    let argument = |mount: &Path, numbers: &[usize]| -> Vec<String> {
        std::iter::once(mount.to_str().unwrap().to_owned())
            .chain(numbers.iter().map(usize::to_string))
            .collect()
    };
    let mut big: Vec<Proc> = (0..BIG_PROCESSES)
        .map(|process| {
            Proc::python(
                format!("A's process {process}"),
                BIG,
                &argument(mount_a, &[process, WRITES_EACH, BLOCK]),
            )
        })
        .collect();
    let mut small: Vec<Proc> = (0..SMALL_PROCESSES)
        .map(|process| {
            Proc::python(
                format!("B's process {process}"),
                SMALL,
                &argument(mount_b, &[process, OPERATIONS_EACH]),
            )
        })
        .collect();

    let before = owner.diagnostics().unwrap();
    let (a_before, b_before) = (work(&rig, a.token), work(&rig, b.token));
    let together = Arc::new(Mutex::new(Together::default()));
    let sampler = {
        let service = rig.harness.service.clone();
        let (token_a, token_b, together) = (a.token, b.token, together.clone());
        Sampler::start(Duration::from_millis(1), move || {
            let count = |token| Some(status(&service, token)?.native?.work?.completed);
            let (Some(a), Some(b)) = (count(token_a), count(token_b)) else {
                return;
            };
            let mut together = together.lock().unwrap();
            together.samples += 1;
            if let Some((last_a, last_b)) = together.last {
                together.both_advanced += u64::from(a > last_a && b > last_b);
            }
            together.last = Some((a, b));
        })
    };
    // All ten start on one line each.
    for process in big.iter_mut().chain(&mut small) {
        process.go();
    }
    // One Commit on B while its operations arrive.
    within("B completed twenty requests", || {
        work(&rig, b.token).completed >= b_before.completed + 20
    });
    let b_at_commit = work(&rig, b.token).completed - b_before.completed;
    let record_b = commit(&rig, b.token, "R6-6 B while its operations arrive");
    let b_after_commit = work(&rig, b.token).completed - b_before.completed;
    assert_eq!(
        rig.harness.status(b.token).binding.effective_root,
        record_b.root
    );

    for process in &mut big {
        assert_eq!(process.release(), [WRITES_EACH], "{}", process.what);
    }
    for process in &mut small {
        assert_eq!(process.release(), [OPERATIONS_EACH], "{}", process.what);
    }
    drop((big, small));
    sampler.finish();
    // Post-reply releases and the kernel's RELEASE requests finish on their
    // own; the counts are read once both connections are quiet.
    let quiet = |token| {
        let mut last = None;
        within("connection quiescent", || {
            let now = work(&rig, token);
            let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
            last = Some(now.completed);
            thread::sleep(Duration::from_millis(10));
            settled
        });
        work(&rig, token)
    };
    let (a_after, b_after) = (quiet(a.token), quiet(b.token));
    let after = owner.diagnostics().unwrap();
    let delta = completed(&before, &after);
    let (samples, both_advanced) = {
        let together = together.lock().unwrap();
        (together.samples, together.both_advanced)
    };
    let dispatch = rig.harness.serving.work().unwrap();
    let (writes, operations) = (
        BIG_PROCESSES * WRITES_EACH,
        SMALL_PROCESSES * OPERATIONS_EACH,
    );
    println!(
        "{what}: a_processes={BIG_PROCESSES} a_writes={writes} a_block={BLOCK} a_completed=+{} a_handoffs=+{} b_processes={SMALL_PROCESSES} b_operations={operations} b_completed=+{} b_handoffs=+{} b_completed_at_commit_call=+{b_at_commit} b_completed_at_commit_return=+{b_after_commit} samples={samples} samples_where_both_advanced={both_advanced}",
        a_after.completed - a_before.completed,
        a_after.handoffs - a_before.handoffs,
        b_after.completed - b_before.completed,
        b_after.handoffs - b_before.handoffs
    );
    println!(
        "{what}: owner_admitted=+{} owner_completed {} outstanding={} peak_queued={} a_retained={} b_retained={} dispatch_retained={} dispatch_parked={} dispatch_queued={}",
        after.admitted - before.admitted,
        classes(delta),
        after.outstanding,
        after.peak_queued,
        a_after.retained,
        b_after.retained,
        dispatch.retained,
        dispatch.parked,
        dispatch.queued
    );
    // Both finished (every process reported its whole sequence above), and
    // each mount completed at least one request per write or operation.
    assert!(
        a_after.completed - a_before.completed >= writes as u64,
        "{what}: A's completed requests are below its {writes} writes"
    );
    assert!(
        b_after.completed - b_before.completed >= operations as u64,
        "{what}: B's completed requests are below its {operations} operations"
    );
    for (class, count) in CLASSES.iter().zip(delta) {
        assert!(
            count > 0,
            "{what}: owner class {class} did not advance: {delta:?}"
        );
    }
    assert_eq!(
        (
            a_after.retained,
            b_after.retained,
            dispatch.retained,
            a_after.terminal,
            b_after.terminal
        ),
        (0, 0, 0, 0, 0),
        "{what}: nothing is retained"
    );
    assert!(
        both_advanced >= 1,
        "{what}: no sample saw both mounts complete requests in one interval ({samples} samples)"
    );

    // What arrived, read back from the daemon.
    for process in 0..BIG_PROCESSES {
        let expected: Vec<u8> = (0..WRITES_EACH)
            .flat_map(|write| [process as u8 + 1, write as u8].repeat(BLOCK / 2))
            .collect();
        let bytes = direct(&mount_a.join(format!("big/p{process}")));
        assert!(
            bytes == expected,
            "{what}: A's file of process {process}: {} bytes of {} expected",
            bytes.len(),
            expected.len()
        );
    }
    let tree_b = native(mount_b);
    let mut expected_b = prepared_b.clone();
    for process in 0..SMALL_PROCESSES {
        for group in (0..OPERATIONS_EACH).step_by(5) {
            let directory = format!("small/p{process}/d-{group:03}");
            let file = format!("small/p{process}/g-{group:03}");
            let (made_directory, made_file) = (
                tree_b
                    .get(directory.as_bytes())
                    .unwrap_or_else(|| panic!("{what}: {directory} is absent")),
                tree_b
                    .get(file.as_bytes())
                    .unwrap_or_else(|| panic!("{what}: {file} is absent")),
            );
            assert_eq!(
                (made_directory.kind, made_directory.mode),
                (DIRECTORY, 0o755)
            );
            assert_eq!(
                (made_file.kind, made_file.mode, made_file.links),
                (FILE, 0o600, 1),
                "{file}"
            );
            assert_eq!(
                made_file.payload,
                format!("small {process} {group:03}\n+\n").into_bytes(),
                "{file}"
            );
            expected_b.insert(directory.into_bytes(), made_directory.clone());
            expected_b.insert(file.into_bytes(), made_file.clone());
        }
        // The parent's time is the clock's.
        let parent = format!("small/p{process}").into_bytes();
        expected_b.insert(parent.clone(), tree_b[&parent].clone());
    }
    assert_same(&expected_b, &tree_b, "B's mount: exactly the operations");
    println!(
        "{what}: both_finished=true a_bytes_exact={} b_tree_exact=true b_commit_published=true",
        writes * BLOCK
    );
    unmount(&rig, &a);
    unmount(&rig, &b);
    rig.finish();
}

/// Base files the two-Workspace readers read, with the seed of their bytes:
/// one to nineteen READ windows of 16 KiB each.
const SHARED: [(&str, u8, usize); 5] = [
    ("docs/guide.md", 10, 5_000),
    ("node_modules/pkg/lib/util.js", 4, 12_000),
    (".cache/tool/data.bin", 6, 20_000),
    ("dist/bundle.js", 9, 33_000),
    ("target/debug/app", 7, 300_017),
];

/// Readers on two Workspaces of one daemon. Six processes on A and six on B
/// read the same five base files, each starting at another file, so the same
/// file and different files are read at once on both mounts, while two more
/// processes write 5 MiB each on B. One of the files has seven bytes of B's
/// own in its second window: B's readers see them, A's readers see the base.
/// Every sequence is finite, every read is compared with its exact bytes by
/// the process that made it, and every process finishes inside the bounded
/// wait. Counts only: no share and no time.
#[test]
fn readers_on_two_workspaces_read_exact_bytes_while_one_is_written() {
    const READERS: usize = 6;
    const TURNS: usize = 20;
    const WRITERS: usize = 2;
    const WRITES_EACH: usize = 40;
    const BLOCK: usize = 128 * 1024;
    const OWN: (&str, usize, &str) = ("dist/bundle.js", 20_000, "LOCAL-B");
    let what = "TWO-WORKSPACE READERS";
    let rig = Rig::new("two-readers");
    let a = rig.mount(1);
    let b = rig.mount(2);
    let _mounted = [Mounted(a.directory.clone()), Mounted(b.directory.clone())];
    let (mount_a, mount_b) = (root(&a), root(&b));
    passed(
        &bash_in(
            COMMAND,
            mount_b,
            &format!(
                "set -euo pipefail; umask 022; mkdir big; printf %s '{}' | dd of={} bs=1 seek={} conv=notrunc status=none",
                OWN.2,
                OWN.0,
                OWN.1
            ),
        ),
        "B's own bytes and directory",
    );
    let wanted = |own: bool| -> Vec<(&str, Vec<u8>)> {
        SHARED
            .iter()
            .map(|&(path, seed, length)| {
                let mut bytes = pattern(seed, length);
                if own && path == OWN.0 {
                    bytes[OWN.1..OWN.1 + OWN.2.len()].copy_from_slice(OWN.2.as_bytes());
                }
                (path, bytes)
            })
            .collect()
    };
    let files = |own: bool| -> Vec<String> {
        SHARED
            .iter()
            .map(|&(path, seed, length)| {
                let patch = if own && path == OWN.0 { OWN.2 } else { "" };
                format!("{path}={seed}={length}={}={patch}", OWN.1)
            })
            .collect()
    };
    let readers = |mount: &Path, own: bool, side: &str| -> Vec<Proc> {
        (0..READERS)
            .map(|process| {
                let mut arguments = vec![
                    mount.to_str().unwrap().to_owned(),
                    TURNS.to_string(),
                    process.to_string(),
                ];
                arguments.extend(files(own));
                Proc::python(format!("{side}'s reader {process}"), READER, &arguments)
            })
            .collect()
    };
    let mut read_a = readers(mount_a, false, "A");
    let mut read_b = readers(mount_b, true, "B");
    let mut write_b: Vec<Proc> = (0..WRITERS)
        .map(|process| {
            let arguments = [
                mount_b.to_str().unwrap().to_owned(),
                process.to_string(),
                WRITES_EACH.to_string(),
                BLOCK.to_string(),
            ];
            Proc::python(format!("B's writer {process}"), BIG, &arguments)
        })
        .collect();

    let owner = rig.harness.owner.client();
    let before = owner.diagnostics().unwrap();
    let (a_before, b_before) = (work(&rig, a.token), work(&rig, b.token));
    let together = Arc::new(Mutex::new(Together::default()));
    let sampler = {
        let service = rig.harness.service.clone();
        let (token_a, token_b, together) = (a.token, b.token, together.clone());
        Sampler::start(Duration::from_millis(1), move || {
            let count = |token| Some(status(&service, token)?.native?.work?.completed);
            let (Some(a), Some(b)) = (count(token_a), count(token_b)) else {
                return;
            };
            let mut together = together.lock().unwrap();
            together.samples += 1;
            if let Some((last_a, last_b)) = together.last {
                together.both_advanced += u64::from(a > last_a && b > last_b);
            }
            together.last = Some((a, b));
        })
    };
    // All fourteen start on one line each.
    for process in read_a.iter_mut().chain(&mut read_b).chain(&mut write_b) {
        process.go();
    }
    // No starvation: every finite sequence ends, each inside the bounded wait,
    // with every one of its reads exact.
    for process in read_a.iter_mut().chain(&mut read_b) {
        assert_eq!(process.release(), [TURNS], "{}", process.what);
    }
    for process in &mut write_b {
        assert_eq!(process.release(), [WRITES_EACH], "{}", process.what);
    }
    drop((read_a, read_b, write_b));
    sampler.finish();
    let quiet = |token| {
        let mut last = None;
        within("connection quiescent", || {
            let now = work(&rig, token);
            let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
            last = Some(now.completed);
            thread::sleep(Duration::from_millis(10));
            settled
        });
        work(&rig, token)
    };
    let (a_after, b_after) = (quiet(a.token), quiet(b.token));
    let after = owner.diagnostics().unwrap();
    let delta = completed(&before, &after);
    let (samples, both_advanced) = {
        let together = together.lock().unwrap();
        (together.samples, together.both_advanced)
    };
    let dispatch = rig.harness.serving.work().unwrap();
    // One OPEN, at least one READ and one RELEASE of every turn reached the
    // daemon: the descriptor is `O_DIRECT`.
    let turns = (READERS * TURNS) as u64;
    println!(
        "{what}: readers_each_mount={READERS} turns_each={TURNS} files={} a_completed=+{} a_handoffs=+{} b_writers={WRITERS} b_writes={} b_block={BLOCK} b_completed=+{} b_handoffs=+{} samples={samples} samples_where_both_advanced={both_advanced}",
        SHARED.len(),
        a_after.completed - a_before.completed,
        a_after.handoffs - a_before.handoffs,
        WRITERS * WRITES_EACH,
        b_after.completed - b_before.completed,
        b_after.handoffs - b_before.handoffs
    );
    println!(
        "{what}: owner_admitted=+{} owner_completed {} outstanding={} peak_queued={} a_retained={} b_retained={} dispatch_retained={} dispatch_parked={} dispatch_queued={}",
        after.admitted - before.admitted,
        classes(delta),
        after.outstanding,
        after.peak_queued,
        a_after.retained,
        b_after.retained,
        dispatch.retained,
        dispatch.parked,
        dispatch.queued
    );
    assert!(
        a_after.completed - a_before.completed >= 3 * turns,
        "{what}: A's completed requests are below three for each of its {turns} turns"
    );
    assert!(
        b_after.completed - b_before.completed >= 3 * turns + (WRITERS * WRITES_EACH) as u64,
        "{what}: B's completed requests are below its turns and writes"
    );
    assert_eq!(
        (
            a_after.retained,
            b_after.retained,
            dispatch.retained,
            a_after.terminal,
            b_after.terminal,
            after.outstanding
        ),
        (0, 0, 0, 0, 0, 0),
        "{what}: nothing is retained"
    );
    assert!(
        both_advanced >= 1,
        "{what}: no sample saw both mounts complete requests in one interval ({samples} samples)"
    );

    // What each Workspace serves now: A the base, B the base with its own
    // seven bytes, and B's written files whole.
    for (mount, own) in [(mount_a, false), (mount_b, true)] {
        for (path, bytes) in wanted(own) {
            assert!(
                direct(&mount.join(path)) == bytes,
                "{what}: {path} own={own}"
            );
        }
    }
    for process in 0..WRITERS {
        let expected: Vec<u8> = (0..WRITES_EACH)
            .flat_map(|write| [process as u8 + 1, write as u8].repeat(BLOCK / 2))
            .collect();
        assert!(
            direct(&mount_b.join(format!("big/p{process}"))) == expected,
            "{what}: B's file of writer {process}"
        );
    }
    println!(
        "{what}: all_finished=true reads_exact={} b_bytes_exact={}",
        2 * turns,
        WRITERS * WRITES_EACH * BLOCK
    );
    unmount(&rig, &a);
    unmount(&rig, &b);
    rig.finish();
}
