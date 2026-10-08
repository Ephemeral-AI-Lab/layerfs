//! Real kernel mounts with the kernel's FUSE control filesystem mounted by
//! the test before Attach: the fusectl half of FP-2, and FP-9.
//!
//! SAFETY RULE (`support/fusectl.rs`). `/sys/fs/fuse/connections` lists the
//! connections of other, protected containers. Nothing here writes under it,
//! lists it or reads another connection: a `fusectl::Connection` is built
//! from one mount's own Ready receipt after its device was checked against
//! the test's own mount-table row, and only that connection's `waiting`,
//! `max_background` and `congestion_threshold` are read.
//!
//! FP-9 staging, hook-free. The rig's Store has a fixed read set of two
//! readers; the test leases both through the Store's public admission
//! (`support/holds.rs`), so a request that needs the Store is parked by the
//! request service and nothing is blocked inside a provider call. Two
//! external python3 processes of the command identity each open one large
//! base file before the hold and, when told, read it through the page cache
//! (no `O_DIRECT`). The files were never read, so the first read of each is a
//! page-cache miss that the kernel fills by readahead.
//!
//! What is observed, by counts: the connection's own `waiting` (requests the
//! kernel holds for it, in flight or queued), the mount's maintained
//! `handoffs`, `admitted`, `parked`, `received` and `completed` through
//! control Status, the Store's read-admission counters, and each process's
//! own output and exit status. The READ opcode count of the mount is exposed
//! only by the drain receipt, so during the hold "one READ" is one handoff
//! since a quiescent baseline whose only issuer is a process blocked in one
//! `read(2)` inside its file's size, with one request waiting for a Store
//! reader; the receipt's READ count is printed at the end. Nothing is timed.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
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
use layerfs_bridge::control::{NativePhase, NativeWork, WorkspaceToken};
use layerfs_fuse::request::Opcode;
use mounted::{mount_entry, COMMAND};
use nix::libc;
use rig::{bash_in, passed, pattern, root, stamp_tree, Rig};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::Path,
    process::{Child, ChildStdin, Command as Process, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(5);
/// How long a request is watched before it is recorded as not having
/// completed while the leases were held.
const HELD: Duration = Duration::from_secs(3);
/// Each large base file: several readahead windows of 128 KiB.
const LARGE: usize = 600 * 1024 + 123;
const SEEDS: [u8; 2] = [11, 29];
const LOCAL: &str = "local/written.txt";
const WRITTEN: &[u8] = b"written through the mount\n";
const APPENDED: &[u8] = b"appended while a readahead READ was parked\n";

/// Opens its file, says so, waits for a line, then reads the file through
/// the page cache: one `read(2)` of a page first, the rest inside the size.
const BUFFERED: &str = r#"
import os, sys
path, size = sys.argv[1], int(sys.argv[2])
fd = os.open(path, os.O_RDONLY)
os.write(1, b"opened\n")
sys.stdin.readline()
data = os.read(fd, 4096)
os.write(1, b"first %d\n" % len(data))
while len(data) < size:
    part = os.read(fd, min(65536, size - len(data)))
    if not part:
        break
    data += part
os.close(fd)
os.write(1, b"data " + data.hex().encode() + b"\n")
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
                "FUSECTL_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// One external python3 process of the command identity, launched by plain
/// exec and registered with nothing, that the test talks to by lines. Its
/// output is read by a thread that ends at end of file. Killed and reaped,
/// bounded, on drop.
struct Talker {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    what: String,
}
impl Talker {
    fn python(what: &str, script: &str, arguments: &[&str]) -> Self {
        let mut child = Process::new("python3")
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
        let output = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            stdin: child.stdin.take(),
            child: Some(child),
            lines,
            what: what.to_owned(),
        }
    }
    /// Its next line within `wait`, if it wrote one.
    fn line(&self, wait: Duration) -> Option<String> {
        self.lines.recv_timeout(wait).ok()
    }
    /// Its next line, which must arrive within the bound.
    fn next(&self) -> String {
        self.line(WAIT)
            .unwrap_or_else(|| panic!("{}: no line within {WAIT:?}", self.what))
    }
    fn tell(&mut self, word: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(word.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }
    fn running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| child.try_wait().unwrap().is_none())
    }
    /// Its exit status, or `None` if it has not exited within `wait`.
    fn exit(&mut self, wait: Duration) -> Option<ExitStatus> {
        let child = self.child.as_mut().expect("process already collected");
        let deadline = Instant::now() + wait;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                self.child = None;
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Talker {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let deadline = Instant::now() + Duration::from_secs(3);
        while matches!(child.try_wait(), Ok(None)) {
            if Instant::now() >= deadline {
                println!(
                    "FUSECTL_GUARD {} (pid {}) was killed and is not reaped",
                    self.what,
                    child.id()
                );
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
/// Expectations that did not hold, reported after the leases are returned
/// and the mount is torn down, so a deviation leaves its whole evidence.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("FUSECTL_DEVIATION {what}");
            self.0.push(what);
        }
    }
    fn done(self, name: &str) {
        assert!(
            self.0.is_empty(),
            "{name}: {} expectations did not hold: {:#?}",
            self.0.len(),
            self.0
        );
    }
}

fn work(rig: &Rig, token: WorkspaceToken) -> NativeWork {
    rig.harness.status(token).native.unwrap().work.unwrap()
}
/// The mount's counters once `reached` holds, or as they are after a second.
/// Never fails; the caller checks what it got.
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
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between.
fn quiet(rig: &Rig, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(rig, token);
        let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
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
fn unhex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0, "hexadecimal length");
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}
/// Per-opcode frame counts out of a Debug-rendered drain receipt.
fn opcodes(receipt: &str) -> Vec<u64> {
    let key = "OpcodeWork { opcodes: [";
    let at = receipt.find(key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
    let end = at + receipt[at..].find(']').unwrap();
    receipt[at..end]
        .split(", ")
        .map(|count| count.parse().unwrap())
        .collect()
}
/// A base of two large files under `big/`, each with its own bytes.
fn large_rig(label: &str) -> Rig {
    Rig::built(label, |source| {
        fs::create_dir(source.join("big")).unwrap();
        for (n, seed) in SEEDS.into_iter().enumerate() {
            let path = source.join(large(n));
            fs::write(&path, pattern(seed, LARGE)).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        stamp_tree(source);
    })
}
fn large(n: usize) -> String {
    format!("big/file-{n}")
}

/// FP-2, fusectl half: with the control filesystem mounted before Attach the
/// connection's abort control is bound, and the kernel's own values for the
/// test's connection equal the Ready receipt. Two mounts of one daemon are
/// two connections, each agreeing with its own receipt. The observed maximum
/// request size of the row is not measured here.
#[test]
fn fp2_the_own_connection_agrees_with_the_ready_receipt_and_its_abort_control_is_bound() {
    fusectl::mount();
    assert!(fusectl::mounted());
    let rig = large_rig("fp2-fusectl");
    let first = rig.mount(1);
    let second = rig.mount(2);
    let guards = [
        Mounted(first.directory.clone()),
        Mounted(second.directory.clone()),
    ];
    let mut connections = Vec::new();
    for ready in [&first, &second] {
        let receipt = ready.receipt;
        assert!(receipt.abort_bound, "{receipt:?}");
        let entry = mount_entry(&ready.directory).expect("kernel mount table entry");
        assert_eq!(
            (entry.filesystem.as_str(), entry.source.as_str()),
            ("fuse", "layerfs"),
            "{entry:?}"
        );
        // Panics unless the mount-table device of this directory equals the
        // receipt's; only then is `<minor>/` of the control filesystem read.
        let connection = fusectl::own(ready);
        assert_eq!(connection.minor, receipt.device_minor);
        assert!(connection.exists(), "own connection directory");
        let (background, congestion) = (
            connection.max_background(),
            connection.congestion_threshold(),
        );
        assert_eq!(
            (background, congestion),
            (
                u64::from(receipt.max_background),
                u64::from(receipt.congestion_threshold)
            ),
            "the kernel's limits of connection {} against {receipt:?}",
            connection.minor
        );
        assert_eq!(
            (receipt.max_background, receipt.congestion_threshold),
            (1, 1),
            "the negotiated profile"
        );
        let native = rig.harness.status(ready.token).native.unwrap();
        assert_eq!(native.phase, NativePhase::Ready);
        assert_eq!(native.ready.as_ref(), Some(ready), "the same receipt");
        quiet(&rig, ready.token);
        println!(
            "FP-2 fusectl: mount={} device={}:{} abort_bound={} receipt(max_background={} congestion_threshold={}) fusectl(max_background={background} congestion_threshold={congestion} waiting={})",
            ready.directory,
            receipt.device_major,
            receipt.device_minor,
            receipt.abort_bound,
            receipt.max_background,
            receipt.congestion_threshold,
            connection.waiting()
        );
        connections.push(connection);
    }
    assert_ne!(
        connections[0].minor, connections[1].minor,
        "two mounts are two connections"
    );
    // A served request leaves the idle connection with nothing waiting.
    for (n, ready) in [&first, &second].into_iter().enumerate() {
        let listed = bash_in(COMMAND, root(ready), "ls big");
        passed(&listed, "list through the mount");
        assert_eq!(listed.stdout, b"file-0\nfile-1\n");
        quiet(&rig, ready.token);
        within("nothing waiting on an idle connection", || {
            connections[n].waiting() == 0
        });
    }
    // Normal unmount of one connection removes its own control directory and
    // leaves the other's values as they were. The abort control is not used.
    rig.unmount(&first);
    within("own connection directory removed", || {
        !connections[0].exists()
    });
    assert!(connections[1].exists());
    assert_eq!(
        (
            connections[1].max_background(),
            connections[1].congestion_threshold()
        ),
        (
            u64::from(second.receipt.max_background),
            u64::from(second.receipt.congestion_threshold)
        )
    );
    rig.unmount(&second);
    within("own connection directory removed", || {
        !connections[1].exists()
    });
    println!(
        "FP-2 fusectl: connections={:?} both_removed_after_normal_unmount=true observed_maximum_request_size=NOT_RUN",
        connections.iter().map(|c| c.minor).collect::<Vec<_>>()
    );
    drop(guards);
    rig.finish();
}

/// FP-9: with every Store reader leased to the test, the first process's
/// cold cached read reaches the daemon as one READ, which parks; the second
/// process's cold cached read of another file is then held by the kernel
/// (`waiting` rises, no second request is handed off); an append to a file
/// created through the mount completes meanwhile; after the leases are
/// returned both processes have exactly their files' bytes.
#[test]
fn fp9_a_second_cold_cached_read_queues_in_the_kernel_behind_a_parked_readahead_read() {
    fusectl::mount();
    let rig = large_rig("fp9-background");
    let ready = rig.mount(1);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    assert_eq!(
        (
            connection.max_background(),
            connection.congestion_threshold()
        ),
        (1, 1)
    );
    passed(
        &bash_in(
            COMMAND,
            root(&ready),
            "set -euo pipefail; umask 022; mkdir local; printf 'written through the mount\\n' > local/written.txt",
        ),
        "before the hold",
    );
    let local = root(&ready).join(LOCAL);
    assert_eq!(direct(&local), WRITTEN);
    let mut checks = Checks::default();
    // Declared before the leases: on any path the leases are returned first,
    // so a blocked process can finish and be reaped.
    let size = LARGE.to_string();
    let mut readers: Vec<Talker> = (0..2)
        .map(|n| {
            let path = root(&ready).join(large(n));
            Talker::python(
                &format!("buffered reader {n}"),
                BUFFERED,
                &[path.to_str().unwrap(), &size],
            )
        })
        .collect();
    for reader in &readers {
        assert_eq!(reader.next(), "opened", "{}", reader.what);
    }
    let mut appends: Vec<Talker> = Vec::new();
    quiet(&rig, token);
    within("nothing waiting before the hold", || {
        connection.waiting() == 0
    });
    let leases = holds::Leases::all(&rig.store, WAIT);
    let (held, idle) = (rig.store.read_work(), work(&rig, token));

    // The first process reads one page: a page-cache miss.
    readers[0].tell("go");
    within("the first READ is parked", || work(&rig, token).parked == 1);
    let one = work(&rig, token);
    let waiting_one = connection.waiting();
    let admission_one = rig.store.read_work();
    assert_eq!(
        (one.handoffs - idle.handoffs, one.admitted, one.received),
        (1, 1, 0),
        "one request, handed off and parked: {idle:?} -> {one:?}"
    );
    assert_eq!(
        (
            admission_one.leased,
            admission_one.waiting,
            admission_one.grants
        ),
        (leases.count(), 1, held.grants),
        "the parked request waits for a reader: {admission_one:?}"
    );

    // The second process reads one page of the other file.
    readers[1].tell("go");
    let deadline = Instant::now() + WAIT;
    let (two, waiting_two) = loop {
        let (now, waiting) = (work(&rig, token), connection.waiting());
        if waiting > waiting_one || now.handoffs - idle.handoffs >= 2 || Instant::now() >= deadline
        {
            break (now, waiting);
        }
        thread::sleep(Duration::from_millis(2));
    };
    // The same state at every one of a hundred further observations.
    let (mut least_waiting, mut most_handoffs, mut most_received) = (waiting_two, two.handoffs, 0);
    for _ in 0..100 {
        let (now, waiting) = (work(&rig, token), connection.waiting());
        least_waiting = least_waiting.min(waiting);
        most_handoffs = most_handoffs.max(now.handoffs);
        most_received = most_received.max(now.received);
        thread::sleep(Duration::from_millis(2));
    }
    let delivered = most_handoffs - idle.handoffs;
    let classification = if delivered == 1 && least_waiting >= 2 && waiting_two > waiting_one {
        "background: the second read is held by the kernel behind the limit of one background request"
    } else if delivered >= 2 {
        "foreground: both reads were delivered to the daemon"
    } else {
        "undetermined: the second read was neither delivered nor counted as waiting"
    };
    let silent: Vec<Option<String>> = readers
        .iter()
        .map(|reader| reader.line(Duration::ZERO))
        .collect();
    println!(
        "FP-9 hold: leases_held={} max_background={} waiting(idle=0 one_reader={waiting_one} two_readers={waiting_two} least_over_100={least_waiting}) mount_handoffs_since_idle(one_reader=1 two_readers={delivered}) admitted={} parked={} received_max={most_received} read_admissions_waiting={} classification={classification}",
        leases.count(),
        connection.max_background(),
        two.admitted,
        two.parked,
        rig.store.read_work().waiting
    );
    checks.that(
        delivered == 1 && least_waiting >= 2 && waiting_two > waiting_one,
        || {
            format!(
                "FP-9: the second cold cached read was not observed queued in the kernel: waiting {waiting_one} -> {waiting_two} (least {least_waiting}), handoffs since idle {delivered}; classification {classification}"
            )
        },
    );
    checks.that(silent.iter().all(Option::is_none), || {
        format!(
            "FP-9: a reader returned from its first read while the leases were held: {silent:?}"
        )
    });

    // Foreground meanwhile: an append to a file created through the mount.
    let before_append = work(&rig, token);
    appends.push(Talker::python(
        "append while parked",
        APPEND,
        &[
            local.to_str().unwrap(),
            std::str::from_utf8(APPENDED).unwrap(),
        ],
    ));
    let appended = appends[0].exit(HELD);
    let after_append = settled(&rig, token, |now| {
        now.completed >= before_append.completed + 2
    });
    let waiting_three = connection.waiting();
    let admission = rig.store.read_work();
    let reading = usize::from(readers[0].running()) + usize::from(readers[1].running());
    println!(
        "FP-9 foreground: append_exit={:?} handoffs={}->{} completed={}->{} parked={} waiting_after={waiting_three} readers_still_blocked={reading} reader_grants={}->{}",
        appended.map(|status| status.code()),
        before_append.handoffs,
        after_append.handoffs,
        before_append.completed,
        after_append.completed,
        after_append.parked,
        held.grants,
        admission.grants
    );
    checks.that(appended.is_some_and(|status| status.success()), || {
        format!("FP-9: the append did not complete while the READ was parked: {appended:?} {after_append:?}")
    });
    checks.that(
        after_append.handoffs >= before_append.handoffs + 2
            && after_append.completed >= before_append.completed + 2,
        || {
            format!(
                "FP-9: the mount did not serve the append's OPEN and WRITE beside its parked READ: {before_append:?} -> {after_append:?}"
            )
        },
    );
    checks.that(
        after_append.parked == 1
            && reading == 2
            && waiting_three >= 2
            && admission.grants == held.grants,
        || {
            format!(
                "FP-9: the two reads did not stay held throughout: {after_append:?}, {reading} running, waiting {waiting_three}, {admission:?}"
            )
        },
    );

    leases.release();
    for (n, reader) in readers.iter_mut().enumerate() {
        assert_eq!(reader.next(), "first 4096", "{}", reader.what);
        let line = reader.next();
        let bytes = unhex(line.strip_prefix("data ").expect("data line"));
        assert!(
            bytes == pattern(SEEDS[n], LARGE),
            "buffered reader {n} returned {} bytes that are not its file's",
            bytes.len()
        );
        match reader.exit(WAIT) {
            Some(status) if status.success() => {}
            other => panic!("{}: {other:?}", reader.what),
        }
    }
    if appends[0].child.is_some() {
        match appends[0].exit(WAIT) {
            Some(status) if status.success() => {}
            other => panic!("append after the leases were returned: {other:?}"),
        }
    }
    assert_eq!(
        direct(&local),
        [WRITTEN, APPENDED].concat(),
        "the appended file"
    );
    quiet(&rig, token);
    within("nothing waiting after the release", || {
        connection.waiting() == 0
    });
    let released = rig.store.read_work();
    assert_eq!(
        (released.leased, released.waiting, released.quarantined),
        (0, 0, 0),
        "{released:?}"
    );
    let last = work(&rig, token);
    assert_eq!((last.retained, last.terminal, last.unadmitted), (0, 0, 0));
    drop((readers, appends));
    let receipt = rig.unmount(&ready);
    let counts = opcodes(&receipt);
    println!(
        "FP-9 after release: both_readers_exact=true bytes_each={LARGE} appended_file_exact=true waiting=0 reader_grants={} receipt_opcodes(read={} open={} write={} release={} flush={})",
        released.grants,
        counts[Opcode::Read as usize],
        counts[Opcode::Open as usize],
        counts[Opcode::Write as usize],
        counts[Opcode::Release as usize],
        counts[Opcode::Flush as usize]
    );
    drop(guard);
    rig.finish();
    checks.done("FP-9");
}
