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
//!    shared canonical cache. Bounded reads of distinct stored inode values
//!    displace the mounted reader's prepared values before the gate is armed;
//!    pack and locator caches remain enabled. An external python3 process of
//!    the command identity looks the file up (or opens the directory) while
//!    the gate is open, and makes
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
#[path = "support/fp21_read_pressure.rs"]
mod read_pressure;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/store_gate.rs"]
mod store_gate;
use layerfs_bridge::control::{
    Activity, ControlCode, MountPhase, MountWork, ReadyMount, Reply, WorkspaceToken,
};
use layerfs_daemon::{
    control::Failure,
    store::{Store, StoreReader},
    OwnerClient,
};
use layerfs_fuse::request::Opcode;
use layerfs_history::HistoryCatalog;
use layerfs_overlay::{CleanupState, KernelMount, KernelMountState, StoredCounts};
use layerfs_persistence::Handles;
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage};
use mounted::{mount_entry, Harness, COMMAND};
use std::{
    fmt::Debug,
    fs::{self},
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::{Child, ChildStdin, Command as Process, ExitStatus, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self},
    time::{Duration, Instant},
};
use store_gate::{Gate, Gated};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(5);
/// Bounded indexed retirement after a complete drain.
const LONG: Duration = Duration::from_secs(20);

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

/// The engine's stored counts beside the open state it keeps in memory.
type Rows = (StoredCounts, layerfs_overlay::OpenStateCounts);
fn engine_rows(harness: &Harness, through: WorkspaceToken) -> Rows {
    let resources = harness.engine_resources(through);
    (resources.counts, resources.open)
}
fn work(harness: &Harness, token: WorkspaceToken) -> MountWork {
    harness.status(token).kernel.unwrap().work.unwrap()
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between. A readiness wait before one attempt.
fn quiet(harness: &Harness, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(harness, token);
        let settled = now.active == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
}
/// One owner job of the test, attempted once after a bounded wait for a free
/// slot of its class, and awaited within the same bound.
fn job<T>(
    client: &OwnerClient,
    route: Route,
    command: impl FnOnce(
        &layerfs_daemon::OwnerClient,
        layerfs_workspace::Route,
    ) -> Result<T, layerfs_daemon::OwnerError>,
) -> T {
    command(client, route).unwrap_or_else(|error| panic!("{error:?}"))
}
/// The engine's retained kernel mount of this namespace, if one is attached.
fn engine_mount(client: &OwnerClient, route: Route) -> Option<KernelMount> {
    job(
        client,
        route,
        |job_client: &layerfs_daemon::OwnerClient, job_route| {
            job_client.kernel_retained_mount(job_route)
        },
    )
}
fn mount_state(client: &OwnerClient, route: Route, mount: KernelMount) -> KernelMountState {
    job(
        client,
        route,
        |job_client: &layerfs_daemon::OwnerClient, job_route| {
            job_client.kernel_state(job_route, mount)
        },
    )
}
/// One maintained point observation of the namespace's close and cleanup.
fn cleanup(client: &OwnerClient, token: WorkspaceToken) -> CleanupState {
    // The Workspace's own file can already be gone: the daemon roster
    // answers for the original namespace and incarnation.

    client
        .observe_cleanup(None, token.namespace, token.workspace.to_bytes())
        .unwrap_or_else(|error| panic!("{error:?}"))
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

// ------------------------------------------------------------ Close retained

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
/// One mounted Workspace over a gated Store with no shared canonical cache, a helper
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
    mount: KernelMount,
    /// The unchanged helper Workspace counts.
    empty: StoredCounts,
    /// The same counts with the mount serving and the caller ready.
    idle: Rows,
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
        let scope = writer
            .history
            .branch_snapshot(fixture.branch)
            .unwrap()
            .unwrap()
            .scope;
        let producer = Storage::new(writer.storage.clone()).unwrap();
        let pressure = read_pressure::prepare(&producer);
        drop(producer);
        let gate = Gate::new();
        let reader_config = fixture.config.clone();
        let reader_gate = gate.clone();
        let factory: layerfs_daemon::store::ReaderFactory = Arc::new(move |_| {
            let read = Handles::open_read_only(
                reader_config.clone(),
                installed::BINDING,
                installed::CURSOR,
            )
            .unwrap();
            let provider: Arc<dyn PackPersistence> = Arc::new(Gated {
                inner: read.storage,
                gate: reader_gate.clone(),
            });
            Ok(StoreReader::new(
                Storage::new(provider).unwrap(),
                Arc::new(read.history),
            ))
        });
        let control = factory(0).unwrap();
        let store = Arc::new(
            Store::new(
                writer.storage,
                Arc::new(writer.history),
                control,
                factory,
                0,
                ReservationBlocks::default(),
                16,
            )
            .unwrap(),
        );
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        let client = harness.owner.client();
        let helper = harness.bind(1);
        let empty = harness.engine(helper);
        let ready = harness.mount(2);
        let guard = Mounted(ready.directory.clone());
        let route = holds::route(&harness.service, ready.token);
        let caller = Caller::spawn(
            held.mode(),
            &Path::new(&ready.directory).join(held.target()),
        );
        quiet(&harness, ready.token);
        read_pressure::displace(&store, scope, ready.token.workspace, &pressure);
        let mount = engine_mount(&client, route).expect("attached engine mount");
        let idle = engine_rows(&harness, ready.token);
        assert_eq!(idle.0.namespaces, empty.namespaces);
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
    fn work(&self) -> MountWork {
        work(&self.harness, self.ready.token)
    }
    fn caller(&mut self) -> &mut Caller {
        self.caller.as_mut().unwrap()
    }
    /// The Workspace is neither revoked nor closed.
    fn live(&self) -> bool {
        engine_mount(&self.client, self.route) == Some(self.mount)
            && mount_state(&self.client, self.route, self.mount) == KernelMountState::Live
            && cleanup(&self.client, self.ready.token) == CleanupState::Live
            && self.harness.engine(self.ready.token).namespaces == 1
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
            cleanup(&self.client, self.ready.token) == CleanupState::Gone
        });
        let mut last = self.harness.engine(self.helper);
        before(
            "sibling Workspace counts remain at their baseline",
            LONG,
            || {
                last = self.harness.engine(self.helper);
                last == self.empty
            },
        );
        checks.that(self.gate.observe() == calls, || {
            format!(
                "{name}: a Store read after the Workspace was closed: {calls:?} -> {:?}",
                self.gate.observe()
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
    let grants = staged.store.read_work().requests;

    // The gate is armed, then the caller makes its one request.
    staged.gate.arm();
    staged.caller().send(held.mode());
    let deadline = Instant::now() + WAIT;
    loop {
        let now = staged.work();
        if staged.gate.observe().holding == 1 && now.active == 1 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "NOT STAGED: {held:?}: mount={now:?} gate={:?} reads={:?} storage={:?} caller={:?}",
            staged.gate.observe(),
            staged.store.read_work(),
            staged.store.reader_storage_diagnostics(2),
            staged.caller().lines.try_recv()
        );
        thread::sleep(Duration::from_millis(2));
    }
    let inside = staged.work();
    let rows = engine_rows(&staged.harness, staged.ready.token);
    let readers = staged
        .harness
        .status(token)
        .local
        .map(|local| local.base_readers);
    println!(
        "{name} held: handoffs={}->{} active={} gate={:?} reader_demands={}->{} engine_rows(idle={:?} held={rows:?}) base_readers={readers:?}",
        ready_work.served,
        inside.served,
        inside.active,
        staged.gate.observe(),
        grants,
        staged.store.read_work().requests,
        staged.idle
    );
    assert_eq!(
        inside.served - ready_work.served,
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
            Err(Failure::Kernel(failure)) => checks.that(
                (failure.code, failure.phase) == (ControlCode::Busy, "unmount:kernel"),
                || format!("{name}: Unmount beside the held request: {failure:?}"),
            ),
            other => panic!("{name}: Unmount beside the held request: {}", brief(&other)),
        }
        let status = staged.harness.status(token);
        let now = staged.work();
        let untouched = status.activity == Activity::Idle
            && staged.harness.phase(token) == MountPhase::Ready
            && (now.active, now.terminal, now.retained) == (1, inside.terminal, 0)
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
    let counts = engine_rows(&staged.harness, staged.ready.token);
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
            let opened = engine_rows(&staged.harness, staged.ready.token);
            checks.that(opened.1.file_handles > staged.idle.1.file_handles, || {
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
                "{name}: released: open=ok file_handles {} -> {} read_exact={} bytes={BYTES}",
                staged.idle.1.file_handles,
                opened.1.file_handles,
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
    assert_eq!((after.retained, after.terminal), (0, 0), "{after:?}");

    // The same Unmount now completes, and the Workspace's rows retire.
    let done = staged.harness.unmount(&staged.ready);
    let receipt = format!("{:?}", done.kernel);
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

use layerfs_workspace::Route;
