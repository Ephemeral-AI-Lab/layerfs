//! Real kernel mounts: a normal Unmount and a forced unmount with a READ
//! INSIDE its Store read. A READ records nothing in the engine, so
//! `revoke_native_mount` cannot see it; what keeps `Revoke` and Close behind
//! it is the kernel's busy answer while its caller lives and the lane's
//! drain predicate afterwards
//! ([native read custody](../../../docs/architecture/73-native-read-custody.md),
//! [forced teardown](../../../docs/architecture/80-forced-teardown.md)).
//!
//! Staging, hook-free. The Store is composed from the public pieces
//! `open_store` composes, with each read session's pack persistence port
//! behind `support/store_gate.rs` and no object cache, so every base demand
//! reaches the provider. An external python3 process of the command identity
//! opens one base file while the gate is open and reads it when told to,
//! after the test armed the gate: the READ makes its one owner visit, takes
//! one reader and is held in that reader's provider call.
//!
//! - With `O_DIRECT` the READ is a synchronous kernel request: its caller is
//!   blocked in read(2), and the kernel answers a plain detach with busy.
//! - Without it the READ is a readahead request of the page cache
//!   (`FUSE_ASYNC_READ` is in the mount profile) and its caller waits
//!   killably for the page. The test kills and reaps that caller; the kernel
//!   then holds no reference for the request, so the product's plain detach
//!   succeeds while the daemon is still inside the Store read. This is the
//!   one case in which only the drain predicate stands before `Revoke`.
//!
//! Observed: the product's replies and stored custody, control Status, the
//! engine's maintained counts and point states through ordinary owner jobs,
//! the Store's read-admission counters, the gate's call counts, the mount
//! table and the caller's own answers. Nothing is timed. Product
//! expectations are collected and reported after the mount is gone.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/store_gate.rs"]
mod store_gate;
use layerfs_bridge::control::{
    AbortDisposition, Activity, ControlCode, DetachDisposition, ForcedOutcome, NativePhase,
    NativeWork, ReadyMount, Reply, Request, TeardownCustody, TeardownStage, WorkspaceToken,
};
use layerfs_daemon::{
    control::Failure,
    store::{ReadLimits, Store, StoreReader},
    Command, Completion, NativeJob, NativeReply, OwnerClient, OwnerError, Response,
};
use layerfs_overlay::{CleanupState, NativeMount, NativeMountState, Route, StoredCounts};
use layerfs_persistence::Handles;
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage};
use mounted::{mount_entry, Harness, COMMAND};
use std::{
    fmt::Debug,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::{Child, ChildStdin, Command as Process, ExitStatus, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use store_gate::{Gate, Gated};

/// Every bounded readiness or completion observation of the test.
const WAIT: Duration = Duration::from_secs(5);
/// Bounded indexed retirement after a complete drain.
const LONG: Duration = Duration::from_secs(10);
const FILE: &str = "cold/held.bin";
const BYTES: usize = 3000;
/// The errnos Linux gives a caller of an aborted connection.
const ABORTED: [&str; 2] = ["ENOTCONN", "ECONNABORTED"];

/// Opens the file its second argument names (with `O_DIRECT` in mode
/// `direct`), then answers one line for each `read` it is sent: the bytes
/// of one pread of the file's head in hex, or the errno of that call.
const CALLER: &str = r#"
import errno, os, sys
mode, path = sys.argv[1], sys.argv[2]
fd = os.open(path, os.O_RDONLY | (os.O_DIRECT if mode == "direct" else 0))
def say(text):
    os.write(1, (text + "\n").encode())
say("held")
while True:
    word = sys.stdin.readline().strip()
    if word != "read":
        break
    try:
        say("ok " + os.pread(fd, 4096, 0).hex())
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
    let mut end = text.len().min(700);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}
/// Product expectations that did not hold. Reported after every process is
/// reaped and every mount is gone, so a deviation leaves its whole evidence.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("STORE_READ_DEVIATION {what}");
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
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "STORE_READ_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
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
    /// Returns once the descriptor is open.
    fn spawn(mode: &str, file: &Path) -> Self {
        let mut child = Process::new("python3")
            .arg("-c")
            .arg(CALLER)
            .arg(mode)
            .arg(file)
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
    /// Sends `read` and does not wait for the answer.
    fn read(&mut self) {
        writeln!(self.input.as_mut().unwrap(), "read").unwrap();
    }
    fn silent(&self) -> bool {
        matches!(self.lines.try_recv(), Err(mpsc::TryRecvError::Empty))
    }
    /// SIGKILL and a bounded reap; `None` if the kernel did not let it die.
    fn kill(&mut self) -> Option<ExitStatus> {
        let mut child = self.child.take().unwrap();
        child.kill().unwrap();
        let status = exited(&mut child, WAIT);
        if status.is_none() {
            self.child = Some(child);
        }
        status
    }
    /// Told to leave: the status it exited with by itself.
    fn leave(mut self) -> ExitStatus {
        writeln!(self.input.as_mut().unwrap(), "exit").unwrap();
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
                println!("STORE_READ_GUARD caller {} is not reaped", child.id());
            }
        }
    }
}

fn job(client: &OwnerClient, route: Route, mut command: Command) -> Completion {
    let deadline = Instant::now() + WAIT;
    let pending = loop {
        match client.try_submit(Some(route), command) {
            Ok(pending) => break pending,
            Err((OwnerError::AdmissionFull, back)) => {
                assert!(Instant::now() < deadline, "owner observation {back:?}");
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
        assert!(Instant::now() < deadline, "owner observation incomplete");
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
fn cleanup(client: &OwnerClient, route: Route) -> CleanupState {
    let done = job(client, route, Command::CleanupState);
    match done.result() {
        Ok(Response::CleanupState(state)) => *state,
        other => panic!("{other:?}"),
    }
}

/// Two consecutive observations with nothing received or admitted and no
/// request completed in between. A readiness wait before one attempt.
fn quiet(harness: &Harness, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = harness.status(token).native.unwrap().work.unwrap();
        let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
}

/// One mounted Workspace over a gated Store, a helper Workspace bound beside
/// it for the engine's daemon-wide counts, and one caller with the base file
/// open, about to be held inside the Store read of its READ.
struct Staged {
    /// Dropped first: lets every held provider call go on.
    open: Open,
    caller: Caller,
    /// A lazy unmount of what an unwinding test left.
    _guard: Mounted,
    ready: ReadyMount,
    helper: WorkspaceToken,
    route: Route,
    mount: NativeMount,
    /// Daemon-wide counts with only the helper bound.
    empty: StoredCounts,
    /// The same counts with the mount serving and the descriptor open.
    idle: StoredCounts,
    client: OwnerClient,
    gate: Arc<Gate>,
    store: Arc<Store>,
    harness: Harness,
    fixture: installed::Fixture,
}
impl Staged {
    fn new(label: &str, mode: &str) -> Self {
        fusectl::mount();
        let fixture = installed::Fixture::built(label, |source| {
            fs::create_dir(source.join("cold")).unwrap();
            fs::set_permissions(source.join("cold"), fs::Permissions::from_mode(0o755)).unwrap();
            fs::write(source.join(FILE), content()).unwrap();
            fs::set_permissions(source.join(FILE), fs::Permissions::from_mode(0o644)).unwrap();
            2
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
        assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
        let route = harness
            .service
            .operation(ready.token)
            .unwrap()
            .workspace()
            .route();
        let caller = Caller::spawn(mode, &Path::new(&ready.directory).join(FILE));
        quiet(&harness, ready.token);
        let mount = engine_mount(&client, route).expect("attached engine mount");
        let idle = harness.engine(helper);
        assert_eq!(idle.namespaces, empty.namespaces + 1);
        Self {
            open: Open(gate.clone()),
            caller,
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
        let status = self.harness.status(self.ready.token);
        status.native.unwrap().work.unwrap()
    }
    fn quiet(&self) {
        quiet(&self.harness, self.ready.token);
    }
    /// Arms the gate and has the caller read: returns once the READ is
    /// inside its Store read, holding one reader and nothing in the engine.
    fn hold(&mut self, name: &str, checks: &mut Checks) -> NativeWork {
        let grants = self.store.read_work().grants;
        self.gate.arm();
        self.caller.read();
        within("the READ is inside its Store read", || {
            let now = self.work();
            self.gate.observe().holding == 1
                && (now.admitted, now.parked, now.received) == (1, 0, 0)
                && self.store.read_work().outstanding == 1
        });
        let held = self.work();
        let rows = self.harness.engine(self.helper);
        let local = self.harness.status(self.ready.token).local;
        let readers = local.as_ref().map(|local| local.base_readers);
        println!(
            "{name} held: admitted={} parked={} running={} gate={:?} reader_grants={}->{} engine_rows(idle={:?} held={rows:?}) base_readers={readers:?}",
            held.admitted,
            held.parked,
            held.running,
            self.gate.observe(),
            grants,
            self.store.read_work().grants,
            self.idle
        );
        checks.that(rows == self.idle && readers == Some(0), || {
            format!(
                "{name}: the READ inside its Store read holds engine rows: {:?} -> {rows:?}, base_readers={readers:?}",
                self.idle
            )
        });
        held
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
    /// The registry still owns a retained session that nothing can settle:
    /// the assembly is dropped with that custody, as `forced_unmount.rs`
    /// abandons its retained mounts.
    fn abandon(self) {
        let Self {
            open,
            caller,
            _guard: guard,
            store,
            harness,
            fixture,
            ..
        } = self;
        drop((open, caller, guard));
        let Harness {
            owner,
            serving,
            service,
            ..
        } = harness;
        drop(service);
        drop(serving);
        owner.stop().unwrap();
        drop(store);
        fixture.cleanup();
    }
}

/// Normal Unmount beside a READ whose caller is blocked in read(2).
/// Contract (architecture 73): "Normal Unmount makes the kernel's reversible
/// detach first, which answers busy while a caller is blocked in the READ".
/// Released, the READ answers the file's bytes (no fence was stopped) and
/// the same Unmount then completes and retires the Workspace's rows.
#[test]
fn unmount_is_busy_beside_a_read_inside_its_store_read_and_completes_after_its_reply() {
    let name = "STORE-READ unmount(busy)";
    let mut checks = Checks::default();
    let mut staged = Staged::new("store-read-busy", "direct");
    let token = staged.ready.token;
    let held = staged.hold(name, &mut checks);

    // Twenty attempts while the READ is held: each is the reversible busy
    // answer with no effect, and the READ is still inside its Store read.
    for round in 0..20 {
        match staged.harness.try_unmount(token) {
            Err(Failure::Native(failure)) => checks.that(
                (failure.code, failure.phase) == (ControlCode::Busy, "unmount:kernel"),
                || format!("{name}: Unmount beside the held READ: {failure:?}"),
            ),
            other => panic!("{name}: Unmount beside the held READ: {}", brief(&other)),
        }
        let status = staged.harness.status(token);
        let now = staged.work();
        checks.that(
            status.activity == Activity::Idle
                && staged.harness.phase(token) == NativePhase::Ready
                && (now.admitted, now.parked, now.terminal) == (1, 0, held.terminal)
                && staged.gate.observe().holding == 1
                && mount_entry(&staged.ready.directory).is_some()
                && staged.caller.silent(),
            || format!("{name}: busy Unmount {round} had an effect: {status:?} {now:?}"),
        );
    }
    checks.that(staged.live(), || {
        format!("{name}: the Workspace was revoked or closed under the reader")
    });
    println!(
        "{name}: 20 x Unmount=Busy@unmount:kernel phase=Ready admitted=1 engine_mount=Live cleanup=Live caller_blocked=true"
    );

    staged.gate.release();
    let answer = staged.caller.line();
    checks.that(answer == format!("ok {}", hex(&content())), || {
        format!("{name}: the READ answered {answer:?}")
    });
    println!(
        "{name}: released: read_answer_exact={} bytes={BYTES}",
        answer == format!("ok {}", hex(&content()))
    );
    let exit = std::mem::replace(
        &mut staged.caller,
        Caller {
            child: None,
            input: None,
            lines: mpsc::channel().1,
        },
    )
    .leave();
    assert!(exit.success(), "{name}: the caller ended {exit:?}");
    staged.quiet();
    let done = staged.harness.unmount(&staged.ready);
    drop(done);
    staged.retired(name, &mut checks);
    println!("{name}: Unmounted; cleanup=Gone; engine counts equal the baseline");
    staged.finish();
    checks.done(name);
}

enum Left {
    Unmounted {
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

/// Normal Unmount past its detach, with the READ still inside its Store
/// read: the case that rests on the drain predicate alone. Contract
/// (architecture 73): Unmount "requires the lane's drain (`received == 0 &&
/// admitted == 0`) before `Revoke` is submitted", and a READ "stays
/// `admitted` from its receive until its future ends, whether it is parked
/// for a reader or inside a Store read". The READ's caller was killed, so
/// its bytes have no reader; what is asserted is that the Unmount stays in
/// its drain with the Workspace live, and completes once the read returns.
#[test]
fn a_detached_unmount_drains_behind_a_read_inside_its_store_read_before_it_revokes() {
    let name = "STORE-READ unmount(drain)";
    let mut checks = Checks::default();
    let mut staged = Staged::new("store-read-drain", "buffered");
    let token = staged.ready.token;
    let held = staged.hold(name, &mut checks);

    // The caller waits killably for the page its readahead READ fills.
    let killed = staged.caller.kill();
    assert!(
        killed.is_some(),
        "{name}: NOT STAGED: the caller of the held READ could not be killed, so the READ was not a background readahead request"
    );
    let after_kill = staged.work();
    assert_eq!(
        (after_kill.admitted, staged.gate.observe().holding),
        (1, 1),
        "{name}: the READ left its Store read when its caller died: {after_kill:?}"
    );

    let (left, worker) = unmount_apart(&staged.harness, token);
    within("detach known and the loops joined", || {
        let native = staged.harness.status(token).native.unwrap();
        let work = native.work.unwrap();
        native.phase == NativePhase::Draining
            && native.detached
            && work.loops_joined == work.loops_configured
    });
    // Detach and joined loops alone revoke nothing: at every one of fifty
    // observations the Unmount is still draining, the READ is still inside
    // its Store read and the Workspace is live.
    for round in 0..50 {
        let status = staged.harness.status(token);
        let native = status.native.unwrap();
        let now = native.work.unwrap();
        assert_eq!(status.activity, Activity::Closing);
        assert_eq!(native.phase, NativePhase::Draining, "{now:?}");
        assert_eq!((now.admitted, now.received), (1, 0), "{now:?}");
        assert_eq!(staged.gate.observe().holding, 1);
        if round % 10 == 0 {
            assert!(staged.live(), "{name}: revoked or closed under the reader");
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(mount_entry(&staged.ready.directory).is_none(), "detached");
    assert!(
        matches!(left.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "{name}: the Unmount returned with the READ inside its Store read"
    );
    let waiting = staged.work();
    println!(
        "{name}: caller_killed={killed:?} phase=Draining detached=true loops_joined={} steady_observations=50 admitted={} running={} parked={} completed={}->{} engine_mount=Live cleanup=Live unmount_returned=false",
        waiting.loops_joined,
        waiting.admitted,
        waiting.running,
        waiting.parked,
        held.completed,
        waiting.completed
    );

    staged.gate.release();
    let done = left
        .recv_timeout(WAIT)
        .expect("the Unmount did not return after the Store read was released");
    worker.join().unwrap();
    match done {
        Left::Unmounted {
            exact,
            earlier,
            closed,
            receipt,
        } => {
            assert!(exact, "{name}: the reply is Unmounted of this token");
            assert_eq!(earlier, 1, "native-owner revocation receipt");
            assert!(closed, "logical Close receipt");
            assert!(receipt.contains("Drained"), "{receipt}");
            println!("{name}: released: reply=Unmounted revocation_receipts=1 close_receipt=true receipt={}", brief(&receipt));
        }
        Left::Retained(custody) => {
            panic!("{name}: released inside the drain window, yet {custody:?}")
        }
        Left::Other(other) => panic!("{name}: {other}"),
    }
    assert!(mount_entry(&staged.ready.directory).is_none());
    assert!(matches!(
        staged
            .harness
            .service
            .execute_control(&Request::Status(token)),
        Err(Failure::Rejected(ControlCode::Missing, _))
    ));
    staged.retired(name, &mut checks);
    println!("{name}: cleanup=Gone; engine counts equal the baseline");
    staged.finish();
    checks.done(name);
}

/// Forced unmount beside a READ inside its Store read. Contract
/// (architecture 73): "one inside a Store read runs to its own result while
/// the drain waits for it"; (architecture 80) `force_drain` "waits, bounded
/// by `drain_wait`, for every loop to be joined and for no received or
/// admitted request to remain, and only then makes one plain `umount2`", a
/// stop there is `Retained` at `Requests`, and "There is no way out of
/// `Retained`". The gate is held past the drain window, so the expected end
/// is `Retained` at `Requests` with the abort written and no detach, the
/// Workspace neither revoked nor closed. The caller's read(2) is ended by
/// the kernel's abort. Releasing the Store read afterwards lets the request
/// leave the lane; nothing else changes.
#[test]
fn force_beside_a_read_inside_its_store_read_stops_at_requests_and_revokes_nothing() {
    let name = "STORE-READ force";
    let mut checks = Checks::default();
    let mut staged = Staged::new("store-read-force", "direct");
    let token = staged.ready.token;
    let held = staged.hold(name, &mut checks);
    let grants = staged.store.read_work().grants;

    // One Force, on this thread: it returns at the end of its drain window.
    let custody = match staged.harness.try_force(token, false) {
        Err(Failure::Retained(custody)) => *custody,
        Ok(done) => panic!(
            "{name}: forced to completion with a READ inside its Store read: {:?}",
            done.reply
        ),
        Err(other) => panic!("{name}: {}", brief(&other)),
    };
    println!("{name}: custody={custody:?}");
    checks.that(
        custody.token == token && custody.stage == TeardownStage::Requests && !custody.detached,
        || format!("{name}: not Retained at Requests, still attached: {custody:?}"),
    );
    checks.that(
        custody.forced.as_ref().is_some_and(|facts| {
            facts.abort == AbortDisposition::Written
                && facts.detach == DetachDisposition::NotAttempted
                && facts.fenced == 0
        }),
        || format!("{name}: forced facts: {:?}", custody.forced),
    );
    checks.that(
        custody.work.is_some_and(|work| {
            (work.received, work.admitted, work.retained) == (0, 1, 0)
                && work.loops_joined == work.loops_configured
                && work.loops_configured != 0
        }),
        || format!("{name}: counters at the stop: {:?}", custody.work),
    );
    // Still inside the Store read; the Workspace is neither revoked nor
    // closed, the aborted mount is still in the table, and no reader was
    // granted since.
    checks.that(
        staged.gate.observe().holding == 1 && staged.store.read_work().outstanding == 1,
        || {
            format!(
                "{name}: the Store read after Force: {:?}",
                staged.gate.observe()
            )
        },
    );
    checks.that(staged.live(), || {
        format!("{name}: the Workspace was revoked or closed under the reader")
    });
    checks.that(mount_entry(&staged.ready.directory).is_some(), || {
        format!("{name}: a detach was made before the drain held")
    });
    checks.that(staged.harness.phase(token) == NativePhase::Retained, || {
        format!("{name}: phase {:?}", staged.harness.phase(token))
    });
    // The blocked caller got the kernel's answer for an aborted connection.
    let answer = staged.caller.line();
    let errno = answer
        .strip_prefix("errno ")
        .and_then(|rest| rest.split(' ').nth(1));
    checks.that(errno.is_some_and(|errno| ABORTED.contains(&errno)), || {
        format!("{name}: the held READ's caller answered {answer:?}, not one of {ABORTED:?}")
    });
    println!(
        "{name}: stage={:?} detached={} forced={:?} gate_holding=1 engine_mount=Live cleanup=Live mount_row=present caller_answer={answer:?}",
        custody.stage, custody.detached, custody.forced
    );

    // The Store read returns after the fence was stopped. The request ends
    // by itself; no teardown step follows it.
    staged.gate.release();
    within("the held READ left the lane", || {
        let now = staged.work();
        now.admitted == now.retained && now.received == 0 && staged.gate.observe().holding == 0
    });
    let after = staged.work();
    let calls = staged.gate.observe();
    println!(
        "{name}: released: lane(admitted={} retained={} completed={}->{}) gate={calls:?} reader_grants={}->{}",
        after.admitted,
        after.retained,
        held.completed,
        after.completed,
        grants,
        staged.store.read_work().grants
    );
    checks.that(
        (after.admitted, after.retained) == (0, 0) && after.completed == held.completed + 1,
        || format!("{name}: the released READ did not complete: {held:?} -> {after:?}"),
    );
    checks.that(
        calls.entered == calls.returned
            && calls.expired == 0
            && staged.store.read_work().grants == grants
            && staged.store.read_work().outstanding == 0,
        || {
            format!(
                "{name}: Store use after the release: {calls:?} {:?}",
                staged.store.read_work()
            )
        },
    );
    // Nothing was published or recorded for the request, and the entry is
    // still retained at the same stage with the Workspace live.
    let rows = staged.harness.engine(staged.helper);
    checks.that(rows == staged.idle && staged.live(), || {
        format!(
            "{name}: engine rows after the release: {:?} -> {rows:?}",
            staged.idle
        )
    });
    for (what, request) in [
        ("Unmount", Request::Unmount(token)),
        (
            "ForceUnmount",
            Request::ForceUnmount {
                token,
                relinquish_unknown: false,
            },
        ),
    ] {
        match staged.harness.service.execute_control(&request) {
            Err(Failure::Retained(again)) => checks.that(
                (again.stage, again.detached, &again.detail, &again.forced)
                    == (
                        custody.stage,
                        custody.detached,
                        &custody.detail,
                        &custody.forced,
                    ),
                || format!("{name}: later {what} answered {again:?}"),
            ),
            other => panic!("{name}: later {what}: {}", brief(&other)),
        }
    }
    checks.that(mount_entry(&staged.ready.directory).is_some(), || {
        format!("{name}: a detach followed the release")
    });
    println!(
        "{name}: after release: phase=Retained stage=Requests engine_rows=idle engine_mount=Live later_calls=same custody, no detach"
    );

    // The product never detaches this mount again: the test reaps its
    // caller and detaches it with one plain umount(8).
    let exit = std::mem::replace(
        &mut staged.caller,
        Caller {
            child: None,
            input: None,
            lines: mpsc::channel().1,
        },
    )
    .leave();
    assert!(exit.success(), "{name}: the caller ended {exit:?}");
    let mut umount = Process::new("umount")
        .arg(&staged.ready.directory)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let detached = exited(&mut umount, WAIT).expect("umount(8) did not return");
    assert!(detached.success(), "{name}: umount: {detached:?}");
    assert!(mount_entry(&staged.ready.directory).is_none());
    staged.abandon();
    checks.done(name);
}

enum Forced {
    Unmounted(Box<ForcedOutcome>, bool),
    Retained(Box<TeardownCustody>),
    Other(String),
}
/// One forced unmount, attempted once on its own thread. The thread ends
/// with the product's own bounded drain; it waits for no thread of the test.
fn force_apart(harness: &Harness, token: WorkspaceToken) -> (Receiver<Forced>, JoinHandle<()>) {
    let service = harness.service.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let request = Request::ForceUnmount {
            token,
            relinquish_unknown: false,
        };
        let left = match service.execute_control(&request) {
            Ok(done) => match &done.reply {
                Reply::ForceUnmounted(closed) if closed.token == token => {
                    Forced::Unmounted(Box::new(closed.outcome.clone()), done.completion.is_some())
                }
                other => Forced::Other(format!("{other:?}")),
            },
            Err(Failure::Retained(custody)) => Forced::Retained(custody),
            Err(other) => Forced::Other(format!("{other:?}")),
        };
        let _ = sender.send(left);
    });
    (receiver, worker)
}

/// Forced unmount whose held READ returns inside the drain window. Contract
/// (architecture 73): "one inside a Store read runs to its own result while
/// the drain waits for it"; (architecture 80) `force_drain` "waits, bounded
/// by `drain_wait`, for every loop to be joined and for no received or
/// admitted request to remain, and only then makes one plain `umount2`".
/// The abort ends the caller's read(2) and the caller leaves, so nothing of
/// the kernel holds the mount; the gate is released while the Force is
/// still inside its window, and the same Force then detaches, revokes and
/// closes.
#[test]
fn force_completes_when_the_read_inside_its_store_read_returns_inside_the_drain_window() {
    let name = "STORE-READ force(released)";
    let mut checks = Checks::default();
    let mut staged = Staged::new("store-read-force-released", "direct");
    let token = staged.ready.token;
    let held = staged.hold(name, &mut checks);
    let grants = staged.store.read_work().grants;

    let started = Instant::now();
    let (left, worker) = force_apart(&staged.harness, token);
    // The kernel's answer to the blocked caller is what tells the test that
    // the abort was written.
    let answer = staged.caller.line();
    let errno = answer
        .strip_prefix("errno ")
        .and_then(|rest| rest.split(' ').nth(1));
    checks.that(errno.is_some_and(|errno| ABORTED.contains(&errno)), || {
        format!("{name}: the held READ's caller answered {answer:?}, not one of {ABORTED:?}")
    });
    let exit = std::mem::replace(
        &mut staged.caller,
        Caller {
            child: None,
            input: None,
            lines: mpsc::channel().1,
        },
    )
    .leave();
    assert!(exit.success(), "{name}: the caller ended {exit:?}");
    // The Force is in its drain: at every observation it has not returned,
    // the READ is still inside its Store read, no detach was made and the
    // Workspace is neither revoked nor closed.
    for round in 0..10 {
        assert!(
            matches!(left.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "{name}: NOT STAGED: the Force returned before the Store read was released, {:?} after it started",
            started.elapsed()
        );
        let now = staged.work();
        assert_eq!((now.admitted, now.received), (1, 0), "{now:?}");
        assert_eq!(staged.gate.observe().holding, 1);
        assert!(mount_entry(&staged.ready.directory).is_some(), "detached");
        if round % 5 == 0 {
            assert!(staged.live(), "{name}: revoked or closed under the reader");
        }
        thread::sleep(Duration::from_millis(2));
    }
    let waiting = staged.work();
    let waited = started.elapsed();
    println!(
        "{name}: caller_answer={answer:?} caller_left=true steady_observations=10 admitted={} loops_joined={}/{} engine_mount=Live cleanup=Live mount_row=present force_returned=false released_after={waited:?}",
        waiting.admitted, waiting.loops_joined, waiting.loops_configured
    );

    staged.gate.release();
    let done = left
        .recv_timeout(WAIT)
        .expect("the Force did not return after the Store read was released");
    worker.join().unwrap();
    match done {
        Forced::Unmounted(outcome, closed) => {
            println!("{name}: released: reply=ForceUnmounted outcome={outcome:?} close_receipt={closed}");
            checks.that(
                outcome.facts.abort == AbortDisposition::Written
                    && outcome.facts.detach == DetachDisposition::Detached
                    && outcome.facts.fenced == 0
                    && closed,
                || format!("{name}: forced facts: {outcome:?}"),
            );
            checks.that(
                (outcome.work.received, outcome.work.admitted, outcome.work.retained) == (0, 0, 0)
                    && outcome.work.completed == held.completed + 1,
                || format!("{name}: the released READ did not complete: {held:?} -> {:?}", outcome.work),
            );
        }
        Forced::Retained(custody) => panic!(
            "{name}: released {waited:?} after the Force started, inside its drain window, yet {custody:?}"
        ),
        Forced::Other(other) => panic!("{name}: {other}"),
    }
    assert!(mount_entry(&staged.ready.directory).is_none());
    assert!(matches!(
        staged
            .harness
            .service
            .execute_control(&Request::Status(token)),
        Err(Failure::Rejected(ControlCode::Missing, _))
    ));
    // The READ took no second reader, and the Workspace's rows retire.
    checks.that(staged.store.read_work().grants == grants, || {
        format!(
            "{name}: reader grants {grants} -> {}",
            staged.store.read_work().grants
        )
    });
    staged.retired(name, &mut checks);
    println!("{name}: cleanup=Gone; engine counts equal the baseline");
    staged.finish();
    checks.done(name);
}
