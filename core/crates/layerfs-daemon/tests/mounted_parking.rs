//! Real kernel mounts: requests parked on Store reader admission (FP-8, and
//! the admission half of FP-34).
//!
//! Staging, hook-free. The rig's Store has a fixed read set of two readers.
//! The test leases both through the Store's public admission
//! (`support/holds.rs`), so nothing is blocked inside a provider call: a
//! request that needs the Store waits for a reader and is parked by the
//! request service. Cold readers are external python3 processes of the
//! command identity that open a base file with `O_DIRECT` and read it whole.
//! Their names were looked up before the hold, so each has exactly one
//! foreground request in flight: its OPEN, which needs the base inode and so
//! a reader. None is a readahead request that the kernel's background limit
//! of one would queue behind another. The tests check that count: one
//! handoff for each parked reader.
//!
//! What is observed: the mount's maintained `received`, `admitted`, `parked`,
//! `handoffs` and `completed` counters through control Status, the Store's
//! read-admission counters, and each process's own exit status and bytes.
//! Nothing is timed. "Did not complete while the leases were held" means a
//! bounded observation of three seconds during which the process had not
//! exited; the leases are then returned and the process must finish.
//!
//! Limits. Which of the two receive loops took a request is not observable:
//! FP-8 shows requests parked with both loops in service, and FP-34 shows
//! both receive units held at once. A request of the sibling Workspace or of
//! the same mount is chosen so that it has no base demand (a file created
//! through that mount); a request that needs base facts would wait for the
//! same readers on any Workspace, because the read set belongs to the Store.
//! The same-mount request is a write: a READ takes a Store read ticket even
//! when its bytes are all local, which the third test records as a limit.
//! The terminal half of FP-34 is not here (`forced_unmount.rs`).
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/holds.rs"]
mod holds;
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
use layerfs_bridge::control::{NativeWork, ReadyMount, Reply, Request, WorkspaceToken};
use layerfs_daemon::{control::Service, Command, OwnerError};
use layerfs_history::WorkspaceId;
use mounted::{mount_entry, COMMAND};
use nix::libc;
use rig::{bash_in, passed, pattern, root, stamp_tree, Rig};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command as Process, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the test.
const WAIT: Duration = Duration::from_secs(5);
/// How long a request is watched before it is recorded as not having
/// completed while the leases were held.
const HELD: Duration = Duration::from_secs(3);
/// Cold base files, one per reader.
const COLD: usize = 20;
const COLD_BYTES: usize = 12_000;
/// The kernel-facing bounds of one mount: handoff slots and receive units.
const HANDOFFS: u32 = 16;
const RECEIVE_UNITS: u32 = 2;
const LOCAL: &str = "local/written.txt";
const WRITTEN: &[u8] = b"written through the mount\n";
const APPENDED: &[u8] = b"appended while readers were parked\n";

/// Opens with `O_DIRECT` and reads to the end: every byte is a READ served
/// by the daemon, never a page the kernel already held. Writes the bytes to
/// standard output.
const READER: &str = r#"
import os, sys
fd = os.open(sys.argv[1], os.O_RDONLY | os.O_DIRECT)
data = b""
while True:
    part = os.read(fd, 16384)
    if not part:
        break
    data += part
os.close(fd)
os.write(1, data)
"#;
/// One append to an existing file: OPEN, one WRITE, close.
const APPEND: &str = r#"
import os, sys
fd = os.open(sys.argv[1], os.O_WRONLY | os.O_APPEND)
if os.write(fd, sys.argv[2].encode()) != len(sys.argv[2]):
    sys.exit(9)
os.close(fd)
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
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "PARKING_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// One external python3 process of the command identity, launched by plain
/// exec and registered with nothing. Killed and reaped, bounded, on drop.
struct Proc {
    child: Option<Child>,
    what: String,
}
impl Proc {
    fn python(what: impl Into<String>, script: &str, arguments: &[&str]) -> Self {
        let child = Process::new("python3")
            .arg("-c")
            .arg(script)
            .args(arguments)
            .uid(COMMAND)
            .gid(COMMAND)
            .current_dir("/")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            child: Some(child),
            what: what.into(),
        }
    }
    fn running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| child.try_wait().unwrap().is_none())
    }
    /// Its exit status and whole standard output, or `None` if it has not
    /// exited within `wait`; it then stays owned and can be waited for again.
    fn wait(&mut self, wait: Duration) -> Option<(ExitStatus, Vec<u8>)> {
        let child = self.child.as_mut().expect("process already collected");
        let deadline = Instant::now() + wait;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                let mut output = Vec::new();
                child
                    .stdout
                    .take()
                    .unwrap()
                    .read_to_end(&mut output)
                    .unwrap();
                self.child = None;
                return Some((status, output));
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    /// Must exit 0 within the bound; returns its standard output.
    fn done(&mut self) -> Vec<u8> {
        match self.wait(WAIT) {
            Some((status, output)) if status.success() => output,
            Some((status, _)) => panic!("{}: {status:?}", self.what),
            None => panic!("{}: still running after {WAIT:?}", self.what),
        }
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
                    "PARKING_GUARD {} (pid {}) was killed and is not reaped",
                    self.what,
                    child.id()
                );
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
/// Product expectations that did not hold. Reported after the leases are
/// returned and the mounts are torn down, so a deviation still leaves its
/// whole evidence and no mount.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("PARKING_DEVIATION {what}");
            self.0.push(what);
        }
    }
    fn done(self, name: &str) {
        assert!(
            self.0.is_empty(),
            "{name}: {} product expectations did not hold: {:#?}",
            self.0.len(),
            self.0
        );
    }
}
/// A second thread calling `sample` until it is finished or dropped, and for
/// at most a minute.
struct Sampler {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Sampler {
    fn start(mut sample: impl FnMut() + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
                sample();
                thread::sleep(Duration::from_millis(1));
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

/// A base of `COLD` files under `cold/`, each with its own bytes.
fn cold_rig(label: &str) -> Rig {
    Rig::built(label, |source| {
        fs::create_dir(source.join("cold")).unwrap();
        for n in 0..COLD {
            let path = source.join(cold(n));
            fs::write(&path, pattern(n as u8, COLD_BYTES)).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        stamp_tree(source);
    })
}
fn cold(n: usize) -> String {
    format!("cold/f-{n:02}")
}
fn work(rig: &Rig, token: WorkspaceToken) -> NativeWork {
    rig.harness.status(token).native.unwrap().work.unwrap()
}
/// The mount's counters once `reached` holds, or as they are after a second:
/// a request's post-reply releases end shortly after its reply. Never fails;
/// the caller checks what it got.
fn settled(rig: &Rig, token: WorkspaceToken, reached: impl Fn(&NativeWork) -> bool) -> NativeWork {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let now = work(rig, token);
        if reached(&now) || Instant::now() >= deadline {
            return now;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn observed(service: &Service, token: WorkspaceToken) -> Option<NativeWork> {
    match service.execute_control(&Request::Status(token)).ok()?.reply {
        Reply::Status(status) => status.native.and_then(|native| native.work),
        _ => None,
    }
}
fn workspace(tag: u8) -> WorkspaceId {
    WorkspaceId::from_authority([tag; 32]).unwrap()
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

/// Two mounts of one daemon with what the hold needs made beforehand: on
/// each a file created through that mount, and on A the cold names looked up
/// (their bytes are not read).
struct Staged {
    /// Dropped first: a lazy unmount of whatever an unwinding test left.
    _guards: [Mounted; 2],
    a: ReadyMount,
    b: ReadyMount,
    rig: Rig,
}
impl Staged {
    fn new(label: &str) -> Self {
        let rig = cold_rig(label);
        let a = rig.mount(1);
        let b = rig.mount(2);
        let guards = [Mounted(a.directory.clone()), Mounted(b.directory.clone())];
        for (ready, warm) in [(&a, "stat cold/* > /dev/null"), (&b, "true")] {
            passed(
                &bash_in(
                    COMMAND,
                    root(ready),
                    &format!(
                        "set -euo pipefail; umask 022; mkdir local; printf 'written through the mount\\n' > {LOCAL}; {warm}"
                    ),
                ),
                "before the hold",
            );
        }
        assert_eq!(direct(&root(&a).join(LOCAL)), WRITTEN);
        assert_eq!(direct(&root(&b).join(LOCAL)), WRITTEN);
        Self {
            _guards: guards,
            a,
            b,
            rig,
        }
    }
    fn path(ready: &ReadyMount, relative: &str) -> PathBuf {
        root(ready).join(relative)
    }
    /// `count` cold readers on A, one file each.
    fn readers(&self, count: usize) -> Vec<Proc> {
        (0..count)
            .map(|n| {
                let path = Self::path(&self.a, &cold(n));
                Proc::python(
                    format!("cold reader {n}"),
                    READER,
                    &[path.to_str().unwrap()],
                )
            })
            .collect()
    }
    fn append(&self, ready: &ReadyMount, what: &str) -> Proc {
        let path = Self::path(ready, LOCAL);
        Proc::python(
            what,
            APPEND,
            &[
                path.to_str().unwrap(),
                std::str::from_utf8(APPENDED).unwrap(),
            ],
        )
    }
    /// After the leases are returned: every reader exits 0 with exactly its
    /// file's bytes.
    fn exact(readers: &mut [Proc]) {
        for (n, reader) in readers.iter_mut().enumerate() {
            let bytes = reader.done();
            assert!(
                bytes == pattern(n as u8, COLD_BYTES),
                "cold reader {n} returned {} bytes that are not its file's",
                bytes.len()
            );
        }
    }
    fn finish(self) {
        unmount(&self.rig, &self.a);
        unmount(&self.rig, &self.b);
        self.rig.finish();
    }
}
fn appended() -> Vec<u8> {
    [WRITTEN, APPENDED].concat()
}

/// FP-8, the row: while four cold reads are parked on A with every Store
/// reader leased to the test, an unrelated request on the same mount and one
/// on sibling B both complete; after the leases are returned every reader
/// has exactly its bytes.
///
/// The same-mount "unrelated request" is the append to a file created
/// through that mount (OPEN and WRITE): a mutation needs no Store reader. No
/// same-mount READ can serve as one, because every READ takes a Store read
/// ticket, whatever its bytes; see
/// `a_read_of_locally_written_bytes_still_waits_for_a_store_reader`.
#[test]
fn fp8_a_sibling_write_and_a_same_mount_write_complete_while_cold_reads_are_parked() {
    const READERS: usize = 4;
    let staged = Staged::new("fp8-writes");
    let (rig, a, b) = (&staged.rig, &staged.a, &staged.b);
    let mut checks = Checks::default();
    // Declared before the leases: on any path the leases are returned first,
    // so a parked process can finish and be reaped.
    let mut readers: Vec<Proc> = Vec::new();
    let mut appends: Vec<Proc> = Vec::new();
    let leases = holds::Leases::all(&rig.store, WAIT);
    let (held, idle) = (rig.store.read_work(), work(rig, a.token));
    readers.extend(staged.readers(READERS));
    within("four requests parked on A", || {
        work(rig, a.token).parked == READERS as u32
    });
    let parked = work(rig, a.token);
    assert_eq!(
        (parked.admitted, parked.received, parked.retained),
        (READERS as u32, 0, 0),
        "{parked:?}"
    );
    assert_eq!(
        parked.handoffs - idle.handoffs,
        READERS as u64,
        "one parked request for each reader"
    );
    assert_eq!((parked.loops_configured, parked.loops_entered), (2, 2));
    let waiting = rig.store.read_work();
    assert_eq!(
        (waiting.leased, waiting.waiting, waiting.grants),
        (leases.count(), READERS, held.grants),
        "every parked request waits for a reader: {waiting:?}"
    );
    assert_eq!(rig.store.workspace_reads(workspace(1)).unwrap(), READERS);
    let b_before = work(rig, b.token);

    appends.push(staged.append(a, "append on A"));
    let on_a = appends[0].wait(HELD);
    let a_after = settled(rig, a.token, |now| now.completed >= parked.completed + 2);
    appends.push(staged.append(b, "append on B"));
    let on_b = appends[1].wait(HELD);
    let b_after = settled(rig, b.token, |now| now.completed > b_before.completed);
    let a_last = work(rig, a.token);
    let still = rig.store.read_work();
    let reading = running(&mut readers);
    println!(
        "FP-8 writes: readers={READERS} leases_held={} parked_on_a={} read_admissions_waiting={} same_mount_append={:?} a_handoffs={}->{} a_completed={}->{} sibling_append={:?} b_completed={}->{} parked_on_a_after={} readers_still_blocked={reading} reader_grants={}->{}",
        leases.count(),
        parked.parked,
        waiting.waiting,
        on_a.as_ref().map(|(status, _)| status.code()),
        parked.handoffs,
        a_after.handoffs,
        parked.completed,
        a_after.completed,
        on_b.as_ref().map(|(status, _)| status.code()),
        b_before.completed,
        b_after.completed,
        a_last.parked,
        held.grants,
        still.grants
    );
    checks.that(
        on_a.as_ref().is_some_and(|(status, _)| status.success()),
        || {
            format!(
            "FP-8: the append on A did not complete while the leases were held: {on_a:?} {a_after:?}"
        )
        },
    );
    checks.that(
        a_after.handoffs >= parked.handoffs + 2 && a_after.completed >= parked.completed + 2,
        || {
            format!(
            "FP-8: A did not serve the append's OPEN and WRITE beside its parked requests: {parked:?} -> {a_after:?}"
        )
        },
    );
    checks.that(
        on_b.as_ref().is_some_and(|(status, _)| status.success()),
        || {
            format!(
            "FP-8: the append on B did not complete while requests were parked on A: {on_b:?} {b_after:?}"
        )
        },
    );
    checks.that(b_after.completed > b_before.completed, || {
        format!("FP-8: B completed no request: {b_before:?} -> {b_after:?}")
    });
    // The unrelated requests completed while the same requests stayed parked
    // and no reader was granted to anyone.
    checks.that(
        a_last.parked >= READERS as u32 && reading == READERS && still.grants == held.grants,
        || {
            format!(
            "FP-8: the cold reads did not stay parked throughout: {a_last:?} {reading} running, {still:?}"
        )
        },
    );

    leases.release();
    Staged::exact(&mut readers);
    for process in &mut appends {
        if process.child.is_some() {
            process.done();
        }
    }
    assert_eq!(direct(&Staged::path(a, LOCAL)), appended(), "A's file");
    assert_eq!(direct(&Staged::path(b, LOCAL)), appended(), "B's file");
    let released = rig.store.read_work();
    assert_eq!(
        (released.leased, released.waiting, released.quarantined),
        (0, 0, 0)
    );
    println!(
        "FP-8 writes: after release readers_exact={READERS} reader_grants={} both_files_exact=true",
        released.grants
    );
    drop((readers, appends));
    staged.finish();
    checks.done("FP-8 writes");
}

/// A recorded LIMIT of the current product, not a requirement: a READ takes
/// a Store read ticket even when its bytes are all local. Every READ window
/// awaits the immutable source (`layerfs-fuse/src/operations/read.rs`,
/// `Custody::window`), so with every Store reader leased the read of a file
/// written wholly through the mount parks beside the cold reads. The proof
/// plan suggested this read as FP-8's lease-free same-mount request; source
/// contradicts that, and `P1-attempt1-linux-mounted_parking` and
/// `P1-attempt2-linux-mounted_parking` are the failing receipts of that
/// original expectation. The row itself is the test above.
///
/// Asserted as it is: with the leases held the read does not return inside
/// the bounded observation, A's `parked` and the Store's waiting read
/// admissions each rise by one and no reader is granted; after the leases
/// are returned the same process returns the exact bytes. The read is made
/// with `O_DIRECT`, so that it is a READ request on A and not a page the
/// kernel still held. A read that returns while the leases are held fails
/// this test: the limit would then be gone, which is new information.
#[test]
fn a_read_of_locally_written_bytes_still_waits_for_a_store_reader() {
    const READERS: usize = 4;
    let staged = Staged::new("fp8-read");
    let (rig, a) = (&staged.rig, &staged.a);
    let mut checks = Checks::default();
    let mut readers: Vec<Proc> = Vec::new();
    let mut local: Vec<Proc> = Vec::new();
    let leases = holds::Leases::all(&rig.store, WAIT);
    let (held, idle) = (rig.store.read_work(), work(rig, a.token));
    readers.extend(staged.readers(READERS));
    within("four requests parked on A", || {
        work(rig, a.token).parked == READERS as u32
    });
    let parked = work(rig, a.token);
    assert_eq!(
        parked.handoffs - idle.handoffs,
        READERS as u64,
        "one parked request for each reader"
    );
    let waiting = rig.store.read_work();
    assert_eq!(
        (waiting.leased, waiting.waiting),
        (leases.count(), READERS),
        "{waiting:?}"
    );

    let path = Staged::path(a, LOCAL);
    local.push(Proc::python(
        "read of the file written through A",
        READER,
        &[path.to_str().unwrap()],
    ));
    let read = local[0].wait(HELD);
    let after = work(rig, a.token);
    let admission = rig.store.read_work();
    let reading = running(&mut readers);
    println!(
        "FP8_LIMIT readers={READERS} leases_held={} local_read_returned_while_held={} observed_for={HELD:?} a_parked={}->{} read_admissions_waiting={}->{} reader_grants={}->{} a_handoffs={}->{} a_completed={}->{} cold_readers_still_blocked={reading}",
        leases.count(),
        read.is_some(),
        parked.parked,
        after.parked,
        waiting.waiting,
        admission.waiting,
        held.grants,
        admission.grants,
        parked.handoffs,
        after.handoffs,
        parked.completed,
        after.completed
    );
    checks.that(read.is_none(), || {
        format!(
            "FP8_LIMIT: the read of locally written bytes returned while every Store reader was leased, so the recorded limit no longer holds: {:?}; {parked:?} -> {after:?}; {admission:?}",
            read.as_ref().map(|(status, output)| (status.code(), output.len()))
        )
    });
    checks.that(
        after.parked == parked.parked + 1 && admission.waiting == waiting.waiting + 1,
        || {
            format!(
                "FP8_LIMIT: the read is not one more request parked on reader admission: A parked {} -> {}, Store read admissions waiting {} -> {}",
                parked.parked, after.parked, waiting.waiting, admission.waiting
            )
        },
    );
    checks.that(
        admission.grants == held.grants && admission.leased == leases.count(),
        || {
            format!(
                "FP8_LIMIT: a reader was granted while all were leased: {held:?} -> {admission:?}"
            )
        },
    );

    leases.release();
    Staged::exact(&mut readers);
    if local[0].child.is_some() {
        let bytes = local[0].done();
        assert_eq!(bytes, WRITTEN, "the read after the leases were returned");
        println!(
            "FP8_LIMIT after release: the same process returned its {} exact bytes local_read_exact=true cold_readers_exact={READERS}",
            bytes.len()
        );
    }
    drop((readers, local));
    staged.finish();
    checks.done("FP8_LIMIT");
}

/// What the sampling thread saw of A.
#[derive(Default)]
struct Bounds {
    observations: u64,
    admitted: u32,
    received: u32,
    parked: u32,
    exceeded: Vec<NativeWork>,
}

/// FP-34, admission half: with every Store reader leased and twenty cold
/// readers on A, sixteen requests are admitted and parked, both receive
/// units are held by callbacks waiting for a handoff slot, and no observation
/// ever shows more; sibling B completes a write meanwhile; after the leases
/// are returned all twenty readers have exactly their bytes.
#[test]
fn fp34_admitted_and_received_units_stay_within_their_bounds_and_a_sibling_progresses() {
    const READERS: usize = 20;
    let staged = Staged::new("fp34-admission");
    let (rig, a, b) = (&staged.rig, &staged.a, &staged.b);
    let mut checks = Checks::default();
    let seen = Arc::new(Mutex::new(Bounds::default()));
    let sampler = {
        let (service, token, seen) = (rig.harness.service.clone(), a.token, seen.clone());
        Sampler::start(move || {
            let Some(now) = observed(&service, token) else {
                return;
            };
            let mut seen = seen.lock().unwrap();
            seen.observations += 1;
            seen.admitted = seen.admitted.max(now.admitted);
            seen.received = seen.received.max(now.received);
            seen.parked = seen.parked.max(now.parked);
            if (now.admitted > HANDOFFS || now.received > RECEIVE_UNITS) && seen.exceeded.len() < 8
            {
                seen.exceeded.push(now);
            }
        })
    };
    let mut readers: Vec<Proc> = Vec::new();
    let mut sibling: Vec<Proc> = Vec::new();
    let leases = holds::Leases::all(&rig.store, WAIT);
    let (held, idle) = (rig.store.read_work(), work(rig, a.token));
    readers.extend(staged.readers(READERS));
    let full = |now: &NativeWork| {
        (now.admitted, now.parked, now.received) == (HANDOFFS, HANDOFFS, RECEIVE_UNITS)
    };
    within(
        "sixteen requests parked and both receive units held",
        || full(&work(rig, a.token)),
    );
    // The same state at every one of a hundred further observations.
    let mut steady = 0;
    for _ in 0..100 {
        let now = work(rig, a.token);
        assert!(
            now.admitted <= HANDOFFS && now.received <= RECEIVE_UNITS,
            "{now:?}"
        );
        steady += u32::from(full(&now));
        thread::sleep(Duration::from_millis(2));
    }
    let saturated = work(rig, a.token);
    assert_eq!(
        saturated.handoffs - idle.handoffs,
        u64::from(HANDOFFS),
        "sixteen requests handed off; the two received ones are not"
    );
    let waiting = rig.store.read_work();
    assert_eq!(steady, 100, "the saturated state is stable: {saturated:?}");
    assert_eq!(
        (waiting.leased, waiting.waiting, waiting.grants),
        (leases.count(), HANDOFFS as usize, held.grants),
        "{waiting:?}"
    );

    let b_before = work(rig, b.token);
    sibling.push(staged.append(b, "append on B"));
    let on_b = sibling[0].wait(HELD);
    let b_after = settled(rig, b.token, |now| now.completed > b_before.completed);
    let a_after = work(rig, a.token);
    let reading = running(&mut readers);
    println!(
        "FP-34 admission: readers={READERS} leases_held={} a_admitted={} a_parked={} a_received={} a_queued={} a_running={} a_retained={} steady_observations={steady} read_admissions_waiting={} sibling_append={:?} b_completed={}->{} a_after=({},{},{}) readers_still_blocked={reading}",
        leases.count(),
        saturated.admitted,
        saturated.parked,
        saturated.received,
        saturated.queued,
        saturated.running,
        saturated.retained,
        waiting.waiting,
        on_b.as_ref().map(|(status, _)| status.code()),
        b_before.completed,
        b_after.completed,
        a_after.admitted,
        a_after.parked,
        a_after.received
    );
    checks.that(
        on_b.as_ref().is_some_and(|(status, _)| status.success())
            && b_after.completed > b_before.completed,
        || {
            format!(
            "FP-34: B did not progress while A was saturated: {on_b:?} {b_before:?} -> {b_after:?}"
        )
        },
    );
    checks.that(full(&a_after) && reading == READERS, || {
        format!("FP-34: A did not stay saturated while B progressed: {a_after:?}, {reading} readers running")
    });

    leases.release();
    Staged::exact(&mut readers);
    if sibling[0].child.is_some() {
        sibling[0].done();
    }
    assert_eq!(direct(&Staged::path(b, LOCAL)), appended(), "B's file");
    within("A drained", || {
        let now = work(rig, a.token);
        now.admitted == 0 && now.received == 0
    });
    sampler.finish();
    let last = work(rig, a.token);
    let seen = seen.lock().unwrap();
    println!(
        "FP-34 admission: sampler observations={} max_admitted={} max_received={} max_parked={} over_bound={} after_release readers_exact={READERS} a_retained={} a_terminal={}",
        seen.observations,
        seen.admitted,
        seen.received,
        seen.parked,
        seen.exceeded.len(),
        last.retained,
        last.terminal
    );
    assert!(
        seen.exceeded.is_empty(),
        "admitted or received above its bound: {:?}",
        seen.exceeded
    );
    assert_eq!(
        (seen.admitted, seen.received),
        (HANDOFFS, RECEIVE_UNITS),
        "both bounds were reached and never passed"
    );
    assert_eq!((last.retained, last.terminal), (0, 0));
    drop(seen);
    drop((readers, sibling));
    staged.finish();
    checks.done("FP-34 admission half");
}

/// `support/holds.rs` itself: a held read set makes a further admission
/// wait, a held Lifecycle pair refuses a third job before effect, and both
/// holds are returned whole, by `release` and by an unwinding holder.
#[test]
fn the_holds_are_exact_and_are_returned_on_every_path() {
    let rig = cold_rig("holds");
    let store = &rig.store;
    let readers = store.read_handles();
    let leases = holds::Leases::all(store, WAIT);
    assert_eq!(leases.count(), readers);
    let work = store.read_work();
    assert_eq!(
        (work.leased, work.waiting, work.assigned),
        (readers, 0, 0),
        "{work:?}"
    );
    let further = store.read_ticket(None).unwrap();
    assert_eq!(store.read_work().waiting, 1, "a further admission waits");
    drop(further);
    assert_eq!(store.read_work().waiting, 0);
    leases.release();
    let free = store.read_ticket(None).unwrap();
    let work = store.read_work();
    assert_eq!((work.leased, work.assigned, work.quarantined), (0, 1, 0));
    drop(free);

    // A holder that unwinds returns its readers to service unquarantined.
    // `resume_unwind` unwinds without the panic hook's message.
    let unwound = {
        let store = store.clone();
        thread::spawn(move || {
            let _leases = holds::Leases::all(&store, WAIT);
            std::panic::resume_unwind(Box::new("the holder unwinds"));
        })
        .join()
    };
    assert!(unwound.is_err());
    let work = store.read_work();
    assert_eq!(
        (work.leased, work.assigned, work.quarantined),
        (0, 0, 0),
        "{work:?}"
    );

    let token = rig.harness.bind(1);
    let route = holds::route(&rig.harness.service, token);
    let owner = rig.harness.owner.client();
    let outstanding = |expected: usize, what: &str| {
        within(what, || {
            owner.diagnostics().unwrap().outstanding == expected
        })
    };
    outstanding(0, "the owner is idle after the bind");
    let lifecycle = holds::Credits::lifecycle(&owner, route, 2, WAIT);
    assert_eq!(lifecycle.count(), 2);
    outstanding(2, "two kept completions");
    match owner.try_submit(Some(route), Command::State) {
        Err((OwnerError::AdmissionFull, Command::State)) => {}
        other => panic!(
            "a third Lifecycle job beside two held credits: {:?}",
            other.map(|_| ())
        ),
    }
    let ordinary = holds::Credits::ordinary(&owner, route, 3, WAIT);
    outstanding(5, "ordinary credits beside the Lifecycle pair");
    lifecycle.release();
    ordinary.release();
    outstanding(0, "every credit returned");
    holds::Credits::lifecycle(&owner, route, 2, WAIT).release();
    outstanding(0, "the Lifecycle pair is free again");
    println!(
        "HOLDS readers={readers} leased_all=true further_admission_waited=true unwinding_holder_quarantined=0 lifecycle_pair_refused_third=true credits_returned=true"
    );
    let closed = rig.harness.try_unmount(token).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    rig.finish();
}
