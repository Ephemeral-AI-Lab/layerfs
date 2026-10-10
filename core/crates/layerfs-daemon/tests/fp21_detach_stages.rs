//! FP-21, stages a real mount can present that no test staged before (R8b
//! track T-H; also R2-STEP-5). Specification section 11.2: detach and joined
//! loops alone permit no revocation or Close, and "Any unestablished item
//! returns Retained with the remaining owner/phase".
//!
//! 1. **Close retained.** The Workspace's owner lane has two Lifecycle slots
//!    (`OwnerConfig::lifecycle_jobs_per_namespace`). The test keeps the
//!    completion of exactly ONE of its own `State` observations. Normal
//!    Unmount then detaches, drains and submits `Revoke`, which takes the
//!    other slot; the product keeps that receipt while it submits Close
//!    (`control/detach.rs`), so Close finds no slot and is refused
//!    unattempted. The entry must be `Retained` at `Close` with the Revoke
//!    receipt and the drain receipt kept.
//! 2. **OPEN** and 3. **READDIR inside a provider read.** The Store is
//!    composed from the public pieces `open_store` composes, with each read
//!    session's pack persistence port behind `support/store_gate.rs` and no
//!    object cache. An external python3 process of the command identity looks
//!    the file up (or opens the directory) while the gate is open, and makes
//!    its `open` (or its listing) after the test armed the gate: the request
//!    is held inside its reader's provider call. Both are synchronous kernel
//!    requests whose caller is blocked in its system call, so the product's
//!    one plain detach attempt is answered busy by the kernel; what is
//!    asserted is that each attempt has no effect, that nothing is revoked,
//!    closed or retired while the consumer is inside the read, and that the
//!    request then ends with its original outcome.
//! 4. **FORGET units behind held Lifecycle credits at the detach.** Both
//!    Lifecycle credits are held, so each FORGET unit the kernel sends (asked
//!    for by reclaim of the test container's own memory cgroup, never a
//!    system-wide cache drop) is parked before its decrement job. Normal
//!    Unmount detaches and joins the loop with the units still parked. None
//!    may be lost and nothing may be retired until they have run.
//!
//! NOT in scope here (plan section 6, each needs an owner ruling or has no
//! producer): the `Join`, `Owner`, `Lane` and `Registry` stages. `Abort`
//! belongs to forced teardown.
//!
//! Every hold is a real resource taken through a public API and returned by
//! a guard on every path. Counts only; nothing is timed.
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
#[allow(dead_code)]
#[path = "support/store_gate.rs"]
mod store_gate;
use layerfs_bridge::control::{
    Activity, ControlCode, NativePhase, NativeWork, ReadyMount, Reply, Request, TeardownCustody,
    TeardownStage, WorkspaceToken,
};
use layerfs_daemon::{
    control::Failure,
    store::{ReadLimits, Store, StoreReader},
    Command, Completion, NativeJob, NativeReply, OwnerClient, OwnerError, Response,
};
use layerfs_fuse::request::Opcode;
use layerfs_overlay::{CleanupState, NativeMount, NativeMountState, Route, StoredCounts};
use layerfs_persistence::Handles;
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage};
use mounted::{mount_entry, Harness, COMMAND};
use rig::{root, stamp_tree, Rig};
use std::{
    fmt::Debug,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command as Process, ExitStatus, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use store_gate::{Gate, Gated};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(5);
/// Bounded indexed retirement after a complete drain.
const LONG: Duration = Duration::from_secs(20);
/// Bound of one reclaim-until-parked observation; how many requests the
/// kernel needs before it evicts an inode is its own business.
const RECLAIM: Duration = Duration::from_secs(30);

fn before(what: &str, wait: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + wait;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn within(what: &str, condition: impl FnMut() -> bool) {
    before(what, WAIT, condition)
}
/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(900);
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
                "FP-21 GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// Product expectations that did not hold, reported after every hold is
/// returned and every mount is gone, so a deviation leaves its whole evidence.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("FP-21 DEVIATION {what}");
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

fn work(harness: &Harness, token: WorkspaceToken) -> NativeWork {
    harness.status(token).native.unwrap().work.unwrap()
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between. A readiness wait before one attempt.
fn quiet(harness: &Harness, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(harness, token);
        let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
}
/// One owner job of the test, attempted once after a bounded wait for a free
/// slot of its class, and awaited within the same bound.
fn job(client: &OwnerClient, route: Route, mut command: Command) -> Completion {
    let deadline = Instant::now() + WAIT;
    let pending = loop {
        match client.try_submit(Some(route), command) {
            Ok(pending) => break pending,
            Err((OwnerError::AdmissionFull, back)) => {
                assert!(
                    Instant::now() < deadline,
                    "owner observation {back:?}: no free slot: {:?}",
                    client.diagnostics()
                );
                command = back;
                thread::sleep(Duration::from_millis(1));
            }
            Err((error, back)) => panic!("owner observation {back:?}: {error:?}"),
        }
    };
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(
            Instant::now() < deadline,
            "owner observation did not complete: {:?}",
            client.diagnostics()
        );
        thread::sleep(Duration::from_millis(1));
    }
}
/// The engine's retained native mount of this namespace, if one is attached.
fn engine_mount(client: &OwnerClient, route: Route) -> Option<NativeMount> {
    let done = job(client, route, Command::Native(NativeJob::RetainedMount));
    match done.result() {
        Ok(Response::Native(NativeReply::RetainedMount(mount))) => *mount,
        other => panic!("{other:?}"),
    }
}
fn mount_state(client: &OwnerClient, route: Route, mount: NativeMount) -> NativeMountState {
    let done = job(client, route, Command::Native(NativeJob::State(mount)));
    match done.result() {
        Ok(Response::Native(NativeReply::State(state))) => *state,
        other => panic!("{other:?}"),
    }
}
/// One maintained point observation of the namespace's close and cleanup.
fn cleanup(client: &OwnerClient, route: Route) -> CleanupState {
    let done = job(client, route, Command::CleanupState);
    match done.result() {
        Ok(Response::CleanupState(state)) => *state,
        other => panic!("{other:?}"),
    }
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
/// One named counter of the text that follows `section` in a Debug-rendered
/// receipt.
fn counter(receipt: &str, section: &str, name: &str) -> u64 {
    let from = receipt
        .find(section)
        .unwrap_or_else(|| panic!("{section}: {receipt}"));
    let key = format!("{name}: ");
    let at = from
        + receipt[from..]
            .find(&key)
            .unwrap_or_else(|| panic!("{name}: {receipt}"));
    receipt[at + key.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

/// What one normal Unmount returned, reduced on its own thread.
enum Left {
    Unmounted {
        /// The reply was exactly `Unmounted` of the token.
        exact: bool,
        earlier: usize,
        closed: bool,
        receipt: String,
    },
    Retained(Box<TeardownCustody>),
    Other(String),
}
/// One normal Unmount, attempted once on its own thread. The thread ends
/// with the product's own bounded drain; it waits for no thread of the test.
fn unmount_apart(harness: &Harness, token: WorkspaceToken) -> (Receiver<Left>, JoinHandle<()>) {
    let service = harness.service.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let left = match service.execute_control(&Request::Unmount(token)) {
            Ok(done) => Left::Unmounted {
                exact: done.reply == Reply::Unmounted(token),
                earlier: done.earlier.len(),
                closed: done.completion.is_some(),
                receipt: format!("{:?}", done.native),
            },
            Err(Failure::Retained(custody)) => Left::Retained(custody),
            Err(other) => Left::Other(format!("{other:?}")),
        };
        let _ = sender.send(left);
    });
    (receiver, worker)
}

// ------------------------------------------------------------ Close retained

/// FP-21, Close. One kept Lifecycle completion of the test leaves `Revoke`
/// the lane's last slot; the product keeps the Revoke receipt while it
/// submits Close, so Close is refused unattempted after the detach, the
/// drain and the revocation. Expected by source (`control/unmount.rs`,
/// `unclosed`): `Retained` at `Close`, detached, with the Revoke receipt and
/// the drain receipt kept in the entry; later Unmount and Attach answer the
/// same custody and submit nothing; returning the test's credit settles
/// nothing by itself.
#[test]
fn fp21_close_is_retained_after_revoke_when_one_lifecycle_completion_is_held() {
    let rig = Rig::new("fp21-close");
    let h = &rig.harness;
    let client = h.owner.client();
    let helper = h.bind(1);
    within("the owner is idle after the bind", || {
        client.diagnostics().unwrap().outstanding == 0
    });
    let empty = h.engine(helper);
    let start = client.diagnostics().unwrap();
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let route = holds::route(&h.service, token);
    let mut file = File::open(root(&ready).join("README.md")).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"# base\n");
    drop(file);
    quiet(h, token);
    let mount = engine_mount(&client, route).expect("attached engine mount");
    assert_eq!(mount_state(&client, route, mount), NativeMountState::Live);
    assert_eq!(cleanup(&client, route), CleanupState::Live);
    let mounted = h.engine(helper);
    assert_eq!(mounted.namespaces, empty.namespaces + 1);

    // Exactly one Lifecycle completion of this lane, kept by the test.
    let credit = holds::Credits::lifecycle(&client, route, 1, WAIT);
    assert_eq!(credit.count(), 1);
    quiet(h, token);
    within("only the test's credit is outstanding", || {
        client.diagnostics().unwrap().outstanding == 1
    });
    let held = client.diagnostics().unwrap();

    // One Unmount, on this thread.
    let custody = match h.try_unmount(token) {
        Err(Failure::Retained(custody)) => *custody,
        Ok(done) => panic!(
            "NOT STAGED: unmounted with one Lifecycle completion held: {:?}",
            done.reply
        ),
        Err(other) => panic!("{}", brief(&other)),
    };
    let stopped = client.diagnostics().unwrap();
    println!("FP-21 close: custody={custody:?}");
    println!(
        "FP-21 close: owner jobs admitted by the Unmount={} outstanding {} -> {} closed_namespaces {} -> {}",
        stopped.admitted - held.admitted,
        held.outstanding,
        stopped.outstanding,
        start.closed_namespaces,
        stopped.closed_namespaces
    );
    assert_eq!(custody.token, token);
    assert_eq!(
        custody.stage,
        TeardownStage::Close,
        "NOT STAGED unless the stop is at Close: {custody:?}"
    );
    assert!(custody.detached, "the kernel detach is known");
    assert!(custody.forced.is_none());
    assert!(!custody.detail.is_empty());
    let at_stop = custody.work.expect("counters at the stopping boundary");
    assert_eq!(
        (at_stop.loops_configured, at_stop.loops_joined),
        (1, 1),
        "{at_stop:?}"
    );
    assert_eq!(
        (at_stop.received, at_stop.admitted, at_stop.retained),
        (0, 0, 0),
        "{at_stop:?}"
    );
    // The Unmount submitted exactly one job, its Revoke: Close was refused
    // before admission. That job's receipt is kept, so it and the test's
    // credit are the two outstanding results of the owner.
    assert_eq!(stopped.admitted - held.admitted, 1, "Revoke only");
    assert_eq!(stopped.outstanding, 2, "the test's credit and the Revoke");
    assert_eq!(stopped.closed_namespaces, start.closed_namespaces);
    // Effects that happened stay happened; nothing is reported as unmounted.
    assert!(mount_entry(&ready.directory).is_none());
    assert!(!root(&ready).exists(), "the drain removed the directory");
    let status = h.status(token);
    assert_eq!(status.activity, Activity::Closing);
    let native = status.native.unwrap();
    assert_eq!(native.phase, NativePhase::Retained);
    assert!(native.detached);
    assert_eq!(native.ready.as_ref(), Some(&ready));
    let kept = h
        .service
        .retained_native(token)
        .unwrap()
        .expect("retained owner");
    println!("FP-21 close: kept={kept}");
    assert!(kept.contains("stage: Close"), "{kept}");
    assert!(
        kept.contains("Drained"),
        "the drain receipt is kept: {kept}"
    );
    // A completion prints its original result: the Revoke's is `Done`.
    assert!(
        kept.contains("[Ok(Native(Done))]"),
        "the Revoke receipt is kept with its original result: {kept}"
    );
    assert!(
        kept.contains("AdmissionFull"),
        "the unattempted Close's original refusal is kept: {kept}"
    );
    // The namespace is not closed, observed through another namespace
    // while both Lifecycle slots of this one are still in use.
    let while_held = h.engine(helper);
    assert_eq!(while_held.namespaces, empty.namespaces + 1);
    // Taken after that observation, which is an owner job of the test.
    let observed = client.diagnostics().unwrap();

    // Later operations answer with the same custody and attempt nothing.
    for request in [Request::Unmount(token), Request::Attach(token)] {
        match h.service.execute_control(&request) {
            Err(Failure::Retained(again)) => assert_eq!(*again, custody, "{request:?}"),
            other => panic!("{request:?}: {}", brief(&other)),
        }
    }
    assert!(h
        .service
        .execute_control(&Request::Locate(token.workspace))
        .is_ok());
    let later = client.diagnostics().unwrap();
    assert_eq!(
        (later.admitted, later.outstanding),
        (observed.admitted, 2),
        "no owner job was replayed by the later replies"
    );

    // Returning the test's credit settles nothing by itself: no hidden
    // retry of Close runs. The freed slot serves the observations below.
    credit.release();
    within("only the kept Revoke receipt is outstanding", || {
        client.diagnostics().unwrap().outstanding == 1
    });
    let released = client.diagnostics().unwrap();
    assert_eq!(released.admitted, later.admitted, "nothing was submitted");
    let state = mount_state(&client, route, mount);
    let closing = cleanup(&client, route);
    let attached = engine_mount(&client, route);
    println!(
        "FP-21 close: after the credit returned: engine_mount={attached:?} mount_state={state:?} cleanup={closing:?}"
    );
    assert_ne!(
        state,
        NativeMountState::Live,
        "the revocation that was acknowledged stays acknowledged"
    );
    assert_ne!(closing, CleanupState::Gone, "the namespace was not closed");
    assert_eq!(h.phase(token), NativePhase::Retained);
    match h.try_unmount(token) {
        Err(Failure::Retained(again)) => assert_eq!(*again, custody),
        other => panic!("{}", brief(&other)),
    }
    let last = h.engine(helper);
    let end = client.diagnostics().unwrap();
    assert_eq!(last.namespaces, empty.namespaces + 1);
    assert_eq!(end.closed_namespaces, start.closed_namespaces);
    assert_eq!(end.outstanding, 1, "the Revoke receipt is still kept");
    assert_ne!(cleanup(&client, route), CleanupState::Gone);
    let serving = h.serving.work().unwrap();
    assert_eq!(serving.mounts, 0, "the drained lane was released");
    println!(
        "FP-21 close: stage=Close detached=true loops_joined=1 unmount_jobs=1(Revoke) close=refused-unattempted kept(Revoke receipt, Drained) later_replies_equal=3 owner_jobs_replayed=0 after_release(mount_state={state:?} cleanup={closing:?} phase=Retained namespaces={}(baseline {}) closed_namespaces={}->{})",
        last.namespaces, empty.namespaces, start.closed_namespaces, end.closed_namespaces
    );
    assert!(client.maintenance_failure().unwrap().is_none());
    drop(guard);
    let closed = h.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
}

// ------------------------------------------- OPEN and READDIR in a provider

const DIRECTORY: &str = "cold";
const FILE: &str = "cold/held.bin";
const NAMES: [&str; 3] = ["a.txt", "b.txt", "held.bin"];
const BYTES: usize = 3000;

/// `open` mode: looks the file up, then for each word it is sent opens it
/// (`open`) or reads its head through that descriptor (`read`). `list` mode:
/// opens the directory, then lists it through that descriptor for each
/// `list`. Every answer is one line; an errno is answered, never raised.
const CALLER: &str = r#"
import errno, os, sys
mode, path = sys.argv[1], sys.argv[2]
def say(text):
    os.write(1, (text + "\n").encode())
fd = -1
if mode == "open":
    os.lstat(path)
else:
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
say("held")
while True:
    word = sys.stdin.readline().strip()
    try:
        if word == "open":
            fd = os.open(path, os.O_RDONLY)
            say("opened")
        elif word == "read":
            say("ok " + os.pread(fd, 4096, 0).hex())
        elif word == "list":
            say("names " + ",".join(sorted(os.listdir(fd))))
        else:
            break
    except OSError as error:
        say("errno %d %s" % (error.errno, errno.errorcode.get(error.errno, "?")))
say("bye")
sys.exit(0)
"#;

fn content() -> Vec<u8> {
    (0..BYTES).map(|n| (n * 31 + 7) as u8).collect()
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 15)] as char);
    }
    text
}
/// Releases the gate on every path, an unwinding test included, so that no
/// product worker stays held behind a failed assertion.
struct Open(Arc<Gate>);
impl Drop for Open {
    fn drop(&mut self) {
        self.0.release();
    }
}
/// The child's exit status, or `None` if it has not exited within `wait`.
fn exited(child: &mut Child, wait: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + wait;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
            _ => return None,
        }
    }
}
/// One `CALLER` process. Killed and reaped, bounded, on drop.
struct Caller {
    child: Option<Child>,
    input: Option<ChildStdin>,
    lines: Receiver<String>,
}
impl Caller {
    /// Returns once the lookup (or the directory's open) has been answered.
    fn spawn(mode: &str, path: &Path) -> Self {
        let mut child = Process::new("python3")
            .arg("-c")
            .arg(CALLER)
            .arg(mode)
            .arg(path)
            .uid(COMMAND)
            .gid(COMMAND)
            .current_dir("/")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        // Ends at the pipe's end of file, which the process's exit or the
        // guard's kill brings; it waits for no other thread.
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut caller = Self {
            child: Some(child),
            input,
            lines,
        };
        assert_eq!(caller.line(), "held");
        caller
    }
    fn line(&mut self) -> String {
        self.lines
            .recv_timeout(WAIT)
            .unwrap_or_else(|error| panic!("caller: no answer within {WAIT:?}: {error}"))
    }
    /// Sends one word and does not wait for the answer.
    fn send(&mut self, word: &str) {
        writeln!(self.input.as_mut().unwrap(), "{word}").unwrap();
    }
    fn silent(&self) -> bool {
        matches!(self.lines.try_recv(), Err(mpsc::TryRecvError::Empty))
    }
    /// Told to leave: the status it exited with by itself.
    fn leave(mut self) -> ExitStatus {
        self.send("exit");
        assert_eq!(self.line(), "bye");
        drop(self.input.take());
        let mut child = self.child.take().unwrap();
        exited(&mut child, WAIT).expect("the caller did not exit when told to")
    }
}
impl Drop for Caller {
    fn drop(&mut self) {
        drop(self.input.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            if exited(&mut child, Duration::from_secs(3)).is_none() {
                println!("FP-21 GUARD caller {} is not reaped", child.id());
            }
        }
    }
}

/// Which request is held inside its Store read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Held {
    Open,
    Readdir,
}
impl Held {
    fn mode(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Readdir => "list",
        }
    }
    fn target(self) -> &'static str {
        match self {
            Self::Open => FILE,
            Self::Readdir => DIRECTORY,
        }
    }
}
/// One mounted Workspace over a gated Store with no object cache, a helper
/// Workspace bound beside it for the engine's daemon-wide counts, and one
/// caller about to be held inside the Store read of its request.
struct Staged {
    /// Dropped first: lets every held provider call go on.
    open: Open,
    caller: Option<Caller>,
    /// A lazy unmount of what an unwinding test left.
    _guard: Mounted,
    ready: ReadyMount,
    helper: WorkspaceToken,
    route: Route,
    mount: NativeMount,
    /// Daemon-wide counts with only the helper bound.
    empty: StoredCounts,
    /// The same counts with the mount serving and the caller ready.
    idle: StoredCounts,
    client: OwnerClient,
    gate: Arc<Gate>,
    store: Arc<Store>,
    harness: Harness,
    fixture: installed::Fixture,
}
impl Staged {
    fn new(label: &str, held: Held) -> Self {
        let fixture = installed::Fixture::built(label, |source| {
            fs::create_dir(source.join(DIRECTORY)).unwrap();
            fs::set_permissions(source.join(DIRECTORY), fs::Permissions::from_mode(0o755)).unwrap();
            for name in NAMES {
                let path = source.join(DIRECTORY).join(name);
                fs::write(&path, content()).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            }
            1 + NAMES.len()
        });
        let config = || fixture.config.clone();
        let writer =
            Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap();
        let gate = Gate::new();
        let readers = (0..2)
            .map(|_| {
                let read = Handles::open_read_only(config(), installed::BINDING, installed::CURSOR)
                    .unwrap();
                let provider: Arc<dyn PackPersistence> = Arc::new(Gated {
                    inner: read.storage,
                    gate: gate.clone(),
                });
                StoreReader::new(Storage::new(provider).unwrap(), Arc::new(read.history))
            })
            .collect();
        let store = Arc::new(
            Store::new(
                writer.storage,
                Arc::new(writer.history),
                readers,
                0,
                ReservationBlocks::default(),
                ReadLimits::default(),
            )
            .unwrap(),
        );
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        let client = harness.owner.client();
        let helper = harness.bind(1);
        within("the owner is idle after the bind", || {
            client.diagnostics().unwrap().outstanding == 0
        });
        let empty = harness.engine(helper);
        let ready = harness.mount(2);
        let guard = Mounted(ready.directory.clone());
        let route = holds::route(&harness.service, ready.token);
        let caller = Caller::spawn(
            held.mode(),
            &Path::new(&ready.directory).join(held.target()),
        );
        quiet(&harness, ready.token);
        let mount = engine_mount(&client, route).expect("attached engine mount");
        let idle = harness.engine(helper);
        assert_eq!(idle.namespaces, empty.namespaces + 1);
        Self {
            open: Open(gate.clone()),
            caller: Some(caller),
            _guard: guard,
            ready,
            helper,
            route,
            mount,
            empty,
            idle,
            client,
            gate,
            store,
            harness,
            fixture,
        }
    }
    fn work(&self) -> NativeWork {
        work(&self.harness, self.ready.token)
    }
    fn caller(&mut self) -> &mut Caller {
        self.caller.as_mut().unwrap()
    }
    /// The Workspace is neither revoked nor closed.
    fn live(&self) -> bool {
        engine_mount(&self.client, self.route) == Some(self.mount)
            && mount_state(&self.client, self.route, self.mount) == NativeMountState::Live
            && cleanup(&self.client, self.route) == CleanupState::Live
            && self.harness.engine(self.helper).namespaces == self.empty.namespaces + 1
    }
    /// After a complete teardown: the namespace reaches `Gone`, the engine's
    /// counts return to what they were before the Workspace existed, and
    /// every Store read call returned before now, with none made since.
    fn retired(&self, name: &str, checks: &mut Checks) {
        let calls = self.gate.observe();
        checks.that(
            calls.entered == calls.returned && (calls.holding, calls.expired) == (0, 0),
            || format!("{name}: Store read calls after the teardown: {calls:?}"),
        );
        before("cleanup state Gone", LONG, || {
            cleanup(&self.client, self.route) == CleanupState::Gone
        });
        let mut last = self.harness.engine(self.helper);
        before("engine counts returned to their baseline", LONG, || {
            last = self.harness.engine(self.helper);
            last == self.empty
        });
        checks.that(self.gate.observe() == calls, || {
            format!(
                "{name}: a Store read after the Workspace was closed: {calls:?} -> {:?}",
                self.gate.observe()
            )
        });
        checks.that(self.store.read_work().outstanding == 0, || {
            format!(
                "{name}: readers after the teardown: {:?}",
                self.store.read_work()
            )
        });
    }
    /// Closes the helper and stops the assembly; no mount may remain.
    fn finish(self) {
        let Self {
            open,
            caller,
            _guard: guard,
            helper,
            store,
            harness,
            fixture,
            ..
        } = self;
        drop((open, caller, guard));
        let closed = harness.try_unmount(helper).unwrap();
        assert_eq!(closed.reply, Reply::Unmounted(helper));
        drop(closed);
        harness.stop();
        drop(store);
        fixture.cleanup();
    }
}

/// Normal Unmount beside one request held inside its Store read, and the
/// request's own end afterwards.
fn unmount_beside(held: Held) {
    let name = format!("FP-21 {held:?} in provider");
    let mut checks = Checks::default();
    let mut staged = Staged::new(
        match held {
            Held::Open => "fp21-open",
            Held::Readdir => "fp21-readdir",
        },
        held,
    );
    let token = staged.ready.token;
    let ready_work = staged.work();
    let grants = staged.store.read_work().grants;

    // The gate is armed, then the caller makes its one request.
    staged.gate.arm();
    staged.caller().send(held.mode());
    within("the request is inside its Store read", || {
        let now = staged.work();
        staged.gate.observe().holding == 1
            && (now.admitted, now.parked, now.received) == (1, 0, 0)
            && staged.store.read_work().outstanding == 1
    });
    let inside = staged.work();
    let rows = staged.harness.engine(staged.helper);
    let readers = staged
        .harness
        .status(token)
        .local
        .map(|local| local.base_readers);
    println!(
        "{name} held: handoffs={}->{} admitted={} parked={} running={} gate={:?} reader_grants={}->{} engine_rows(idle={:?} held={rows:?}) base_readers={readers:?}",
        ready_work.handoffs,
        inside.handoffs,
        inside.admitted,
        inside.parked,
        inside.running,
        staged.gate.observe(),
        grants,
        staged.store.read_work().grants,
        staged.idle
    );
    assert_eq!(
        inside.handoffs - ready_work.handoffs,
        1,
        "NOT STAGED: exactly one request was handed off since the caller was ready"
    );
    assert!(staged.caller().silent(), "the caller is in its system call");
    // The visit recorded nothing: the lane alone accounts for the request.
    checks.that(rows == staged.idle && readers == Some(0), || {
        format!(
            "{name}: the request inside its Store read holds engine rows: {:?} -> {rows:?}, base_readers={readers:?}",
            staged.idle
        )
    });

    // Ten Unmounts beside the held request. Each makes its one plain detach
    // attempt, which the kernel answers busy while the caller is in its
    // system call: no effect, nothing revoked, closed or retired.
    for round in 0..10 {
        match staged.harness.try_unmount(token) {
            Err(Failure::Native(failure)) => checks.that(
                (failure.code, failure.phase) == (ControlCode::Busy, "unmount:kernel"),
                || format!("{name}: Unmount beside the held request: {failure:?}"),
            ),
            other => panic!("{name}: Unmount beside the held request: {}", brief(&other)),
        }
        let status = staged.harness.status(token);
        let now = staged.work();
        let untouched = status.activity == Activity::Idle
            && staged.harness.phase(token) == NativePhase::Ready
            && (now.admitted, now.parked, now.terminal, now.retained) == (1, 0, inside.terminal, 0)
            && now.completed == inside.completed
            && staged.gate.observe().holding == 1
            && mount_entry(&staged.ready.directory).is_some()
            && staged.caller().silent();
        checks.that(untouched, || {
            format!("{name}: busy Unmount {round} had an effect: {status:?} {now:?}")
        });
    }
    checks.that(staged.live(), || {
        format!("{name}: the Workspace was revoked or closed under the consumer")
    });
    let counts = staged.harness.engine(staged.helper);
    checks.that(counts == staged.idle, || {
        format!(
            "{name}: engine rows changed across the detach attempts: {:?} -> {counts:?}",
            staged.idle
        )
    });
    println!(
        "{name}: 10 x Unmount=Busy@unmount:kernel phase=Ready admitted=1 gate_holding=1 engine_mount=Live cleanup=Live engine_rows=idle caller_blocked=true"
    );

    // The read returns: the request ends with its original outcome.
    staged.gate.release();
    let answer = staged.caller().line();
    match held {
        Held::Open => {
            checks.that(answer == "opened", || {
                format!("{name}: the OPEN answered {answer:?}")
            });
            quiet(&staged.harness, token);
            let opened = staged.harness.engine(staged.helper);
            checks.that(opened.owner_details > staged.idle.owner_details, || {
                format!(
                    "{name}: the deciding visit recorded no descriptor: {:?} -> {opened:?}",
                    staged.idle
                )
            });
            staged.caller().send("read");
            let bytes = staged.caller().line();
            checks.that(bytes == format!("ok {}", hex(&content())), || {
                format!("{name}: the read through the opened descriptor answered {bytes:?}")
            });
            println!(
                "{name}: released: open=ok owner_details {} -> {} read_exact={} bytes={BYTES}",
                staged.idle.owner_details,
                opened.owner_details,
                bytes == format!("ok {}", hex(&content()))
            );
        }
        Held::Readdir => {
            let expected = format!("names {}", NAMES.join(","));
            checks.that(answer == expected, || {
                format!("{name}: the listing answered {answer:?}, not {expected:?}")
            });
            println!("{name}: released: listing={answer:?}");
        }
    }
    let exit = staged.caller.take().unwrap().leave();
    assert!(exit.success(), "{name}: the caller ended {exit:?}");
    quiet(&staged.harness, token);
    let after = staged.work();
    assert_eq!(
        (after.retained, after.terminal, after.unadmitted),
        (0, 0, 0),
        "{after:?}"
    );

    // The same Unmount now completes, and the Workspace's rows retire.
    let done = staged.harness.unmount(&staged.ready);
    let receipt = format!("{:?}", done.native);
    drop(done);
    let frames = opcodes(&receipt);
    let count = |opcode: Opcode| frames[opcode as usize];
    println!(
        "{name}: Unmounted: receipt_opcodes(lookup={} open={} read={} release={} opendir={} readdir={} releasedir={})",
        count(Opcode::Lookup),
        count(Opcode::Open),
        count(Opcode::Read),
        count(Opcode::Release),
        count(Opcode::Opendir),
        count(Opcode::Readdir),
        count(Opcode::Releasedir)
    );
    match held {
        Held::Open => {
            assert_eq!(count(Opcode::Open), 1, "one OPEN in all: {receipt}");
            assert_eq!(count(Opcode::Release), 1, "{receipt}");
            assert_eq!(count(Opcode::Opendir) + count(Opcode::Readdir), 0);
        }
        Held::Readdir => {
            assert_eq!(count(Opcode::Opendir), 1, "one OPENDIR in all: {receipt}");
            assert!(count(Opcode::Readdir) >= 1, "{receipt}");
            assert_eq!(count(Opcode::Open) + count(Opcode::Read), 0);
        }
    }
    staged.retired(&name, &mut checks);
    println!("{name}: cleanup=Gone; engine counts equal the baseline");
    staged.finish();
    checks.done(&name);
}

/// FP-21, OPEN inside its Store read across detach attempts. The deciding
/// visit has not run, so the engine holds nothing of the request; the
/// kernel's busy answer while the caller is in `open` and the lane's count
/// are what stand before `Revoke` and Close.
#[test]
fn fp21_unmount_beside_an_open_inside_its_store_read_retires_nothing_and_the_open_then_succeeds() {
    unmount_beside(Held::Open);
}

/// FP-21, READDIR inside the Store read of its window's inherited names,
/// across detach attempts.
#[test]
fn fp21_unmount_beside_a_readdir_inside_its_store_read_retires_nothing_and_the_listing_is_exact() {
    unmount_beside(Held::Readdir);
}

// ------------------------------------------------- FORGET behind credits

/// Reclaim of the test container's own memory cgroup, and nothing wider.
struct Reclaim(PathBuf);
impl Reclaim {
    /// Declared precondition, not a product effect: this container's cgroup
    /// interface must be writable so reclaim can be requested for it. The
    /// file is the `memory.reclaim` of the cgroup this process is a member
    /// of, read from its own membership record.
    fn own() -> Self {
        let membership = fs::read_to_string("/proc/self/cgroup").unwrap();
        let relative = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .unwrap_or_else(|| panic!("precondition: a cgroup v2 membership: {membership}"));
        let file = Path::new("/sys/fs/cgroup")
            .join(relative.trim_start_matches('/'))
            .join("memory.reclaim");
        let remount = Process::new("mount")
            .args(["-o", "remount,rw", "/sys/fs/cgroup"])
            .output()
            .unwrap();
        assert!(
            remount.status.success() && file.exists(),
            "precondition: container-scoped {} is required: {remount:?}",
            file.display()
        );
        println!(
            "FP-21 reclaim scope: membership={:?} file={}",
            membership.trim(),
            file.display()
        );
        Self(file)
    }
    /// One reclaim request to the kernel, for more than the cgroup can give
    /// back so that its dentry and inode lists are walked to the end. The
    /// kernel's own answer to the request is not a product result.
    fn request(&self) {
        let _ = OpenOptions::new()
            .write(true)
            .open(&self.0)
            .and_then(|mut file| file.write_all(b"1G"));
    }
}

const UNITS: [&str; 3] = ["unit-0", "unit-1", "unit-2"];

/// FP-21, FORGET units parked behind held Lifecycle credits when the mount
/// is detached. Three names are looked up once each; with both credits held
/// the kernel's three FORGET units are admitted and parked before their
/// decrement jobs. Normal Unmount detaches and joins the loop; while the
/// units are parked the Unmount stays in its drain, every lookup row is
/// still in the engine and the namespace is live. When the credits return
/// inside the drain window each unit runs its own decrement job, and only
/// then do Revoke and Close run: no unit is lost and none is unadmitted.
#[test]
fn fp21_forget_units_parked_behind_lifecycle_credits_at_detach_all_run_before_revocation() {
    let reclaim = Reclaim::own();
    let rig = Rig::built("fp21-forget", |source| {
        for name in UNITS {
            fs::write(source.join(name), format!("{name}\n")).unwrap();
        }
        stamp_tree(source);
    });
    let h = &rig.harness;
    let client = h.owner.client();
    let helper = h.bind(1);
    within("the owner is idle after the bind", || {
        client.diagnostics().unwrap().outstanding == 0
    });
    let empty = h.engine(helper);
    let start = client.diagnostics().unwrap();
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let route = holds::route(&h.service, token);
    quiet(h, token);
    let baseline = h.engine(helper);
    assert_eq!(work(h, token).forget_units, 0);
    let mut inodes = Vec::new();
    for name in UNITS {
        inodes.push(fs::symlink_metadata(root(&ready).join(name)).unwrap().ino());
    }
    quiet(h, token);
    let looked = h.engine(helper);
    // Each referenced inode has its lookup row and its file-custody row.
    assert_eq!(
        looked.owner_details - baseline.owner_details,
        2 * UNITS.len() as u64,
        "the lookup and custody rows of three kernel inodes: {baseline:?} {looked:?}"
    );
    let units = UNITS.len() as u64;
    let mut checks = Checks::default();
    let credits = holds::Credits::lifecycle(&client, route, 2, WAIT);

    // The kernel evicts the three inodes: three FORGET units, all parked.
    let mut requests = 0;
    before(
        "three FORGET units are parked before their jobs",
        RECLAIM,
        || {
            requests += 1;
            reclaim.request();
            let now = work(h, token);
            now.forget_units == units && now.parked as u64 == units && now.received == 0
        },
    );
    let parked = work(h, token);
    assert_eq!(
        (parked.admitted as u64, parked.retained, parked.unadmitted),
        (units, 0, 0),
        "{parked:?}"
    );
    assert_eq!(
        h.engine(helper).owner_details,
        looked.owner_details,
        "nothing is decremented while the units are parked"
    );
    let staged = client.diagnostics().unwrap().admitted;
    println!(
        "FP-21 forget staged: inodes={inodes:?} reclaim_requests={requests} forget_units={} parked={} admitted={} completed={} lifecycle_credits_held={} owner_details(baseline={} looked={})",
        parked.forget_units,
        parked.parked,
        parked.admitted,
        parked.completed,
        credits.count(),
        baseline.owner_details,
        looked.owner_details
    );

    let (left, worker) = unmount_apart(h, token);
    within("detach known and the loop joined", || {
        let native = h.status(token).native.unwrap();
        let work = native.work.unwrap();
        native.phase == NativePhase::Draining
            && native.detached
            && work.loops_joined == work.loops_configured
    });
    // Detach and the joined loop retire nothing: at every one of thirty
    // observations the Unmount is still draining, the three units are still
    // parked and every lookup row is still in the engine.
    let (mut steady, mut observations) = (0, 0);
    for round in 0..30 {
        let status = h.status(token);
        let native = status.native.unwrap();
        let now = native.work.unwrap();
        assert_eq!(status.activity, Activity::Closing);
        assert_eq!(native.phase, NativePhase::Draining, "{now:?}");
        assert_eq!(
            (
                now.loops_joined,
                now.admitted as u64,
                now.parked as u64,
                now.received,
                now.forget_units,
                now.unadmitted
            ),
            (1, units, units, 0, units, 0),
            "{now:?}"
        );
        assert_eq!(now.completed, parked.completed, "no unit ran: {now:?}");
        if round % 10 == 0 {
            // An owner job of the test, on the helper's lane.
            let counts = h.engine(helper);
            observations += 1;
            assert_eq!(counts.namespaces, empty.namespaces + 1);
            assert_eq!(
                counts.owner_details, looked.owner_details,
                "no lookup row was retired or lost: {counts:?}"
            );
        }
        steady += 1;
        thread::sleep(Duration::from_millis(2));
    }
    assert!(mount_entry(&ready.directory).is_none(), "detached");
    assert!(
        matches!(left.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "the Unmount has not returned"
    );
    let waiting = client.diagnostics().unwrap();
    // The only owner jobs since the staging are the test's own
    // observations: no decrement, Revoke or Close was submitted.
    assert_eq!(
        waiting.admitted - staged,
        observations,
        "no owner job of the product was submitted while the units were parked"
    );
    assert_eq!(waiting.closed_namespaces, start.closed_namespaces);
    let submitted = waiting.admitted;
    println!(
        "FP-21 forget: phase=Draining detached=true loops_joined=1 steady_observations={steady} parked={units} owner_details={} namespaces=live unmount_returned=false owner_jobs_since_staged={}(the test's observations)",
        looked.owner_details,
        waiting.admitted - staged
    );

    // The credits return inside the drain window.
    credits.release();
    let done = left
        .recv_timeout(WAIT)
        .expect("the Unmount did not return after the credits were released");
    worker.join().unwrap();
    let receipt = match done {
        Left::Unmounted {
            exact,
            earlier,
            closed,
            receipt,
        } => {
            assert!(exact, "the reply is Unmounted of this token: {receipt}");
            assert_eq!(earlier, 1, "native-owner revocation receipt");
            assert!(closed, "logical Close receipt");
            receipt
        }
        Left::Retained(custody) => panic!("released inside the window, yet {custody:?}"),
        Left::Other(other) => panic!("{other}"),
    };
    assert!(receipt.contains("Drained"), "{receipt}");
    let after = client.diagnostics().unwrap();
    let frames = opcodes(&receipt);
    let (forgets, batched) = (
        frames[Opcode::Forget as usize],
        frames[Opcode::BatchForget as usize],
    );
    let lane = "work: MountWork {";
    let (completed, retained, admitted) = (
        counter(&receipt, lane, "completed"),
        counter(&receipt, lane, "retained"),
        counter(&receipt, lane, "admitted"),
    );
    let (unit_count, unadmitted) = (
        counter(&receipt, "OpcodeWork {", "forget_units"),
        counter(&receipt, "OpcodeWork {", "unadmitted"),
    );
    println!(
        "FP-21 forget released: reply=Unmounted owner_jobs_admitted={} (three decrements, Revoke, Close) lane(completed={}->{completed} retained={retained} admitted={admitted}) forget_units={unit_count} unadmitted={unadmitted} forget_frames={forgets} frames_with_several_units={batched} receipt={}",
        after.admitted - submitted,
        parked.completed,
        brief(&receipt)
    );
    // None lost: each unit completed its own decrement, none was retained
    // with a failure and none met terminal admission.
    checks.that(completed == parked.completed + units, || {
        format!(
            "FP-21 forget: {} units completed, not {units}: {receipt}",
            completed - parked.completed
        )
    });
    assert_eq!((retained, admitted), (0, 0), "{receipt}");
    assert_eq!((unit_count, unadmitted), (units, 0), "{receipt}");
    // Each unit was its own owner job, before the Revoke and the Close.
    checks.that(after.admitted - submitted == units + 2, || {
        format!(
            "FP-21 forget: {} owner jobs between the release and the reply, not {}",
            after.admitted - submitted,
            units + 2
        )
    });
    assert!(mount_entry(&ready.directory).is_none());
    assert!(!root(&ready).exists());
    assert!(matches!(
        h.service.execute_control(&Request::Status(token)),
        Err(Failure::Rejected(ControlCode::Missing, _))
    ));

    // The complete drain: bounded indexed retirement until Gone.
    let mut states = Vec::new();
    before("cleanup state Gone", LONG, || {
        let state = cleanup(&client, route);
        if states.last() != Some(&state) {
            states.push(state);
        }
        state == CleanupState::Gone
    });
    let mut last = h.engine(helper);
    before("engine counts returned to their baseline", LONG, || {
        last = h.engine(helper);
        last == empty
    });
    let end = client.diagnostics().unwrap();
    println!(
        "FP-21 forget: cleanup_states={states:?} counts_equal_baseline=true closed_namespaces={}->{}",
        start.closed_namespaces, end.closed_namespaces
    );
    assert_eq!(end.closed_namespaces, start.closed_namespaces + 1);
    assert!(client.maintenance_failure().unwrap().is_none());
    drop(guard);
    let closed = h.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
    checks.done("FP-21 FORGET units behind Lifecycle credits");
}
