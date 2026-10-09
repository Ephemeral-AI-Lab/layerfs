//! Real kernel mounts: forced teardown against what callers hold (FP-33 held
//! case, FP-19-FS, FP-22-FS and the terminal half of FP-34).
//!
//! Staging, hook-free. The control filesystem is mounted by the test before
//! Attach, so each connection's abort control is bound. A reference is an
//! external python3 process of the command identity whose working directory,
//! or one open descriptor, is in the mount; it answers one line for each line
//! it is sent, so the test decides when it makes its next filesystem call. A
//! parked request is a cold `O_DIRECT` open of a base file while the test
//! leases every reader of the Store through its public admission
//! (`support/holds.rs`): nothing is blocked inside a provider call.
//!
//! What is observed: the product's own reply and stored custody, the mount
//! table row of this mount namespace, the test's own connection entry in the
//! control filesystem (stat only), control Status, the Store's read-admission
//! counters, and each external process's own answers and exit status. There
//! is no syscall trace: "one abort write, one detach" is the product's
//! receipt and its unchanged mount row, not an independent count. A process
//! counts as not signalled when it exits by its own status and its handlers
//! for the catchable termination signals saw none.
//!
//! A forced teardown that stops aborted and still mounted leaves a kernel
//! mount the product never detaches again. The test then reaps its own
//! processes and detaches it with one plain umount(8); a drop guard does a
//! lazy one if the test unwinds. Nothing is timed.
//!
//! The safety rule of `support/fusectl.rs` binds this file: nothing here
//! writes under `/sys/fs/fuse/connections` or reads another connection.
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
use layerfs_bridge::control::{
    AbortDisposition, Activity, CommitKnowledge, ControlCode, DetachDisposition, ForcedFacts,
    ForcedOutcome, NativePhase, NativeWork, Reply, Request, TeardownCustody, TeardownStage,
    WorkspaceToken,
};
use layerfs_daemon::control::{Failure, Success};
use mounted::{mount_entry, Harness, COMMAND};
use rig::{bash_in, passed, pattern, root, stamp_tree, Rig};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        process::{CommandExt, ExitStatusExt},
    },
    path::Path,
    process::{Child, ChildStdin, Command as Process, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the test.
const WAIT: Duration = Duration::from_secs(5);
/// Cold base files, one per reader.
const COLD: usize = 20;
const COLD_BYTES: usize = 12_000;
/// The kernel-facing bounds of one mount: handoff slots and receive units.
const HANDOFFS: usize = 16;
const RECEIVE_UNITS: usize = 1;
/// The errnos Linux gives a caller of an aborted connection.
const ABORTED: [&str; 2] = ["ENOTCONN", "ECONNABORTED"];

/// Counts the catchable termination signals it receives, takes the reference
/// its first argument names, then answers one line for each line it is sent:
/// `probe` makes one filesystem call that needs the daemon (a lookup of a
/// name never looked up before, or an `O_DIRECT` read of the held
/// descriptor) and reports its result; `exit` reports the signals seen and
/// leaves with status 0.
const HOLDER: &str = r#"
import errno, os, signal, sys
seen = []
for number in (signal.SIGHUP, signal.SIGINT, signal.SIGQUIT, signal.SIGTERM,
               signal.SIGUSR1, signal.SIGUSR2, signal.SIGALRM):
    signal.signal(number, lambda n, frame: seen.append(n))
mode, mount = sys.argv[1], sys.argv[2]
fd = -1
if mode == "cwd":
    os.chdir(mount)
else:
    fd = os.open(os.path.join(mount, "README.md"), os.O_RDONLY | os.O_DIRECT)
def say(text):
    os.write(1, (text + "\n").encode())
say("held")
probes = 0
while True:
    word = sys.stdin.readline().strip()
    if word == "probe":
        probes += 1
        try:
            if mode == "cwd":
                os.stat("holder-probe-%d" % probes)
                say("ok")
            else:
                say("ok %d" % len(os.pread(fd, 4096, 0)))
        except OSError as error:
            say("errno %d %s" % (error.errno, errno.errorcode.get(error.errno, "?")))
    else:
        break
say("signals %d" % len(seen))
sys.exit(0)
"#;
/// Opens with `O_DIRECT` and reads to the end. Reports the bytes read, or the
/// errno of the call that failed and exit status 7, with the signals seen.
const READER: &str = r#"
import errno, os, signal, sys
seen = []
for number in (signal.SIGHUP, signal.SIGINT, signal.SIGQUIT, signal.SIGTERM,
               signal.SIGUSR1, signal.SIGUSR2, signal.SIGALRM):
    signal.signal(number, lambda n, frame: seen.append(n))
try:
    fd = os.open(sys.argv[1], os.O_RDONLY | os.O_DIRECT)
    total = 0
    while True:
        part = os.read(fd, 16384)
        if not part:
            break
        total += len(part)
    os.close(fd)
    os.write(1, ("read %d signals %d\n" % (total, len(seen))).encode())
except OSError as error:
    name = errno.errorcode.get(error.errno, "?")
    os.write(1, ("errno %d %s signals %d\n" % (error.errno, name, len(seen))).encode())
    sys.exit(7)
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
            println!("FORCED_DEVIATION {what}");
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
                "FORCED_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
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
/// Guard path only: kill, then a bounded reap.
fn reap(child: &mut Child, what: &str) {
    let _ = child.kill();
    if exited(child, Duration::from_secs(3)).is_none() {
        println!(
            "FORCED_GUARD {what} (pid {}) was killed and is not reaped",
            child.id()
        );
    }
}
/// One external process of the command identity, launched by plain exec and
/// registered with nothing. Killed and reaped, bounded, on drop.
struct Proc {
    child: Option<Child>,
    what: String,
}
impl Proc {
    fn start(what: impl Into<String>, mut command: Process, directory: &Path) -> Self {
        let child = command
            .uid(COMMAND)
            .gid(COMMAND)
            .current_dir(directory)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            child: Some(child),
            what: what.into(),
        }
    }
    fn python(what: impl Into<String>, script: &str, argument: &Path) -> Self {
        let mut command = Process::new("python3");
        command.arg("-c").arg(script).arg(argument);
        Self::start(what, command, Path::new("/"))
    }
    /// `bash -c` with the given working directory.
    fn bash(what: impl Into<String>, directory: &Path, script: &str) -> Self {
        let mut command = Process::new("bash");
        command.arg("-c").arg(script);
        Self::start(what, command, directory)
    }
    fn running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }
    /// Its exit status and whole standard output, or `None` if it has not
    /// exited within `wait`; it then stays owned and is reaped on drop.
    fn wait(&mut self, wait: Duration) -> Option<(ExitStatus, Vec<u8>)> {
        let child = self.child.as_mut().expect("process already collected");
        let status = exited(child, wait)?;
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut output)
            .unwrap();
        self.child = None;
        Some((status, output))
    }
}
impl Drop for Proc {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            reap(&mut child, &self.what);
        }
    }
}
/// One `HOLDER` process. Killed and reaped, bounded, on drop.
struct Holder {
    child: Option<Child>,
    input: Option<ChildStdin>,
    lines: Receiver<String>,
    what: String,
}
impl Holder {
    /// `mode` is `cwd` or `descriptor`. Returns once the reference is held.
    fn spawn(mode: &str, mount: &Path) -> Self {
        let mut child = Process::new("python3")
            .arg("-c")
            .arg(HOLDER)
            .arg(mode)
            .arg(mount)
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
        let mut holder = Self {
            child: Some(child),
            input,
            lines,
            what: format!("holder ({mode})"),
        };
        assert_eq!(holder.line(), "held", "{}", holder.what);
        holder
    }
    fn line(&mut self) -> String {
        self.lines
            .recv_timeout(WAIT)
            .unwrap_or_else(|error| panic!("{}: no answer within {WAIT:?}: {error}", self.what))
    }
    /// One line sent, one answer read, bounded.
    fn ask(&mut self, word: &str) -> String {
        writeln!(self.input.as_mut().unwrap(), "{word}").unwrap();
        self.line()
    }
    fn pid(&self) -> u32 {
        self.child.as_ref().unwrap().id()
    }
    fn alive(&mut self) -> bool {
        matches!(self.child.as_mut().unwrap().try_wait(), Ok(None))
    }
    /// Told to leave: its last answer and the status it exited with by itself.
    fn leave(mut self) -> (String, ExitStatus) {
        let last = self.ask("exit");
        drop(self.input.take());
        let mut child = self.child.take().unwrap();
        match exited(&mut child, WAIT) {
            Some(status) => (last, status),
            None => {
                reap(&mut child, &self.what);
                panic!("{}: did not exit when told to", self.what)
            }
        }
    }
}
impl Drop for Holder {
    fn drop(&mut self) {
        drop(self.input.take());
        if let Some(mut child) = self.child.take() {
            reap(&mut child, &self.what);
        }
    }
}
/// The errno name of a `probe` answer that failed.
fn errno(answer: &str) -> Option<&str> {
    let mut words = answer.split(' ');
    (words.next() == Some("errno"))
        .then(|| words.nth(1))
        .flatten()
}

/// This mount namespace's whole mount-table row of `directory`: mount id,
/// parent, device, root, options and filesystem.
fn row(directory: &str) -> Option<String> {
    let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
    table
        .lines()
        .rev()
        .find(|line| {
            line.split_once(" - ")
                .is_some_and(|(before, _)| before.split(' ').nth(4) == Some(directory))
        })
        .map(str::to_owned)
}
fn work(rig: &Rig, token: WorkspaceToken) -> NativeWork {
    rig.harness.status(token).native.unwrap().work.unwrap()
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between. A readiness wait before one attempt.
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
/// What one forced unmount ended as.
enum Ended {
    Unmounted(Box<ForcedOutcome>, Box<Success>),
    /// Stopped after an effect; the registry keeps the exact owner.
    Retained(Box<TeardownCustody>),
}
/// One forced terminal unmount, attempted once.
fn force(harness: &Harness, token: WorkspaceToken) -> Ended {
    match harness.try_force(token, false) {
        Ok(done) => match &done.reply {
            Reply::ForceUnmounted(closed) => {
                assert_eq!(closed.token, token);
                Ended::Unmounted(Box::new(closed.outcome.clone()), Box::new(done))
            }
            other => panic!("forced unmount replied {other:?}"),
        },
        Err(Failure::Retained(custody)) => Ended::Retained(custody),
        Err(other) => panic!("forced unmount: {}", brief(&other)),
    }
}
/// The fields of a stored custody that never change; `detached` and `work`
/// are read live from the connection's maintained counters.
fn stable(
    custody: &TeardownCustody,
) -> (WorkspaceToken, TeardownStage, &str, Option<&ForcedFacts>) {
    (
        custody.token,
        custody.stage,
        custody.detail.as_str(),
        custody.forced.as_ref(),
    )
}
/// Every later terminal or attach call on a retained entry, each once.
fn later(harness: &Harness, token: WorkspaceToken) -> Vec<(&'static str, TeardownCustody)> {
    [
        (
            "ForceUnmount",
            Request::ForceUnmount {
                token,
                relinquish_unknown: false,
            },
        ),
        (
            "ForceUnmount(relinquish_unknown)",
            Request::ForceUnmount {
                token,
                relinquish_unknown: true,
            },
        ),
        ("Unmount", Request::Unmount(token)),
        ("Attach", Request::Attach(token)),
    ]
    .into_iter()
    .map(
        |(name, request)| match harness.service.execute_control(&request) {
            Err(Failure::Retained(custody)) => (name, *custody),
            other => panic!("{name} on a retained entry: {}", brief(&other)),
        },
    )
    .collect()
}
/// A custody stopped at the one detach: aborted and still mounted.
fn aborted_and_mounted(custody: &TeardownCustody, token: WorkspaceToken) -> bool {
    custody.token == token
        && custody.stage == TeardownStage::Detach
        && !custody.detached
        && custody.forced.as_ref().is_some_and(|facts| {
            facts.abort == AbortDisposition::Written
                && facts.detach == DetachDisposition::Busy
                && facts.commit == CommitKnowledge::Absent
        })
}
/// One plain umount(8) of an aborted mount, launched and reaped by the test.
fn plain_umount(directory: &str) -> ExitStatus {
    let mut child = Process::new("umount")
        .arg(directory)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    match exited(&mut child, WAIT) {
        Some(status) => status,
        None => {
            reap(&mut child, "umount");
            panic!("umount {directory} did not return within {WAIT:?}")
        }
    }
}
/// The registry still owns a retained session that nothing can settle. The
/// kernel mount was detached by the test; the assembly is dropped with that
/// custody, as `mounted_commit_failures.rs` drops its retained Commits.
fn abandon(rig: Rig) {
    let Rig {
        fixture,
        store,
        harness,
        ..
    } = rig;
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

/// FP-33, held case. An external process holds `mode` (its working
/// directory, or one open descriptor) in the mount; Force aborts the
/// connection and its one plain detach is refused by the kernel.
fn held_reference(name: &'static str, label: &str, mode: &str) {
    fusectl::mount();
    let rig = Rig::new(label);
    let ready = rig.mount(1);
    let _guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let mut checks = Checks::default();
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    let mount = root(&ready).to_owned();
    let mut holder = Holder::spawn(mode, &mount);
    let pid = holder.pid();
    // The mount serves the holder's own call before the teardown.
    let served = holder.ask("probe");
    assert!(
        served == "errno 2 ENOENT" || served.starts_with("ok "),
        "{name}: the holder's call before the teardown: {served}"
    );
    quiet(&rig, token);
    let before = rig.harness.status(token);
    assert_eq!(before.activity, Activity::Idle);
    let before_row = row(&ready.directory).expect("mount row");
    println!(
        "{name} before: holder_pid={pid} holds={mode} holder_call={served:?} abort_bound={} minor={} own_connection={} phase={:?} epoch={}",
        ready.receipt.abort_bound,
        connection.minor,
        connection.exists(),
        before.native.as_ref().unwrap().phase,
        before.epoch
    );

    let custody = match force(&rig.harness, token) {
        Ended::Retained(custody) => *custody,
        Ended::Unmounted(outcome, _) => {
            panic!("{name}: forced past a held {mode}: {outcome:?}")
        }
    };
    println!("{name} custody={custody:?}");
    println!(
        "{name} retained_owner={}",
        brief(&rig.harness.service.retained_native(token))
    );
    checks.that(aborted_and_mounted(&custody, token), || {
        format!("{name}: not Retained at Detach with abort Written and detach Busy: {custody:?}")
    });
    // The local drain held before the one detach: every loop joined and no
    // request left.
    checks.that(
        custody.work.is_some_and(|work| {
            work.received == 0
                && work.admitted == 0
                && work.retained == 0
                && work.loops_joined == work.loops_configured
                && work.loops_configured != 0
        }),
        || {
            format!(
                "{name}: the local drain did not hold before the detach: {:?}",
                custody.work
            )
        },
    );
    let after_row = row(&ready.directory);
    checks.that(after_row.as_deref() == Some(before_row.as_str()), || {
        format!("{name}: the mount row changed: {before_row:?} -> {after_row:?}")
    });
    let status = rig.harness.status(token);
    let native = status.native.as_ref().unwrap();
    checks.that(
        native.phase == NativePhase::Retained
            && !native.detached
            && status.activity == Activity::Closing,
        || {
            format!(
                "{name}: status after the stop: phase={:?} detached={} activity={:?}",
                native.phase, native.detached, status.activity
            )
        },
    );
    // The holder is alive and its next call on the mount fails by the
    // kernel's own answer for an aborted connection.
    let alive = holder.alive();
    let refused = holder.ask("probe");
    checks.that(alive, || format!("{name}: the holder did not survive"));
    checks.that(errno(&refused).is_some_and(|errno| ABORTED.contains(&errno)), || {
        format!("{name}: the holder's call after the abort answered {refused:?}, not one of {ABORTED:?}")
    });
    println!(
        "{name} after_force: stage={:?} abort={:?} detach={:?} detached={} fenced={:?} mount_row=unchanged:{} own_connection={} phase={:?} activity={:?} holder_alive={alive} holder_call={refused:?}",
        custody.stage,
        custody.forced.as_ref().map(|facts| facts.abort),
        custody.forced.as_ref().map(|facts| facts.detach),
        custody.detached,
        custody.forced.as_ref().map(|facts| facts.fenced),
        after_row.as_deref() == Some(before_row.as_str()),
        connection.exists(),
        native.phase,
        status.activity
    );

    // Every later call returns the stored custody and makes no effect.
    let again = later(&rig.harness, token);
    for (call, repeated) in &again {
        checks.that(
            stable(repeated) == stable(&custody) && !repeated.detached,
            || format!("{name}: {call} returned another custody: {repeated:?}"),
        );
    }
    let repeated_status = rig.harness.status(token);
    let repeated_row = row(&ready.directory);
    checks.that(
        repeated_row.as_deref() == Some(before_row.as_str())
            && repeated_status.epoch == status.epoch
            && repeated_status.native.as_ref().map(|native| native.phase)
                == Some(NativePhase::Retained),
        || {
            format!(
                "{name}: a later call had an effect: row {repeated_row:?}, epoch {} -> {}",
                status.epoch, repeated_status.epoch
            )
        },
    );
    let still = holder.ask("probe");
    println!(
        "{name} later_calls={:?} equal_custody={} mount_row=unchanged:{} epoch_unchanged={} holder_call={still:?}",
        again.iter().map(|(call, _)| *call).collect::<Vec<_>>(),
        again
            .iter()
            .all(|(_, repeated)| stable(repeated) == stable(&custody)),
        repeated_row.as_deref() == Some(before_row.as_str()),
        repeated_status.epoch == status.epoch
    );

    // The holder leaves by itself; nothing signalled it.
    let (signals, exit) = holder.leave();
    checks.that(
        exit.code() == Some(0) && exit.signal().is_none() && signals == "signals 0",
        || format!("{name}: the holder ended {exit:?} after {signals:?}"),
    );
    // Nothing changes once the reference is gone: no detach is made now
    // that it would succeed.
    let gone = later(&rig.harness, token);
    let gone_row = row(&ready.directory);
    let gone_status = rig.harness.status(token);
    checks.that(
        gone.iter()
            .all(|(_, repeated)| stable(repeated) == stable(&custody) && !repeated.detached)
            && gone_row.as_deref() == Some(before_row.as_str())
            && gone_status.epoch == status.epoch,
        || {
            format!(
                "{name}: something changed after the holder exited: row {gone_row:?}, epoch {} -> {}, {gone:?}",
                status.epoch, gone_status.epoch
            )
        },
    );
    println!(
        "{name} after_holder_exit: holder_exit={:?} {signals:?} equal_custody={} mount_row=unchanged:{} own_connection={} phase={:?}",
        exit.code(),
        gone.iter()
            .all(|(_, repeated)| stable(repeated) == stable(&custody)),
        gone_row.as_deref() == Some(before_row.as_str()),
        connection.exists(),
        gone_status.native.as_ref().map(|native| native.phase)
    );

    // Teardown by the test: the product never detaches this mount again.
    let detached = plain_umount(&ready.directory);
    assert!(detached.success(), "{name}: umount: {detached:?}");
    assert!(mount_entry(&ready.directory).is_none());
    within("own connection directory removed", || !connection.exists());
    println!(
        "{name} teardown: by_test=plain umount(8) status={:?} mount_row=None own_connection=absent registry_phase={:?} deviations={}",
        detached.code(),
        rig.harness.phase(token),
        checks.0.len()
    );
    abandon(rig);
    checks.done(name);
}

#[test]
fn fp33_a_held_working_directory_leaves_the_mount_aborted_and_retained() {
    held_reference("FP-33 cwd", "fp33-cwd", "cwd");
}

#[test]
fn fp33_a_held_descriptor_leaves_the_mount_aborted_and_retained() {
    held_reference("FP-33 descriptor", "fp33-descriptor", "descriptor");
}

/// A base of `COLD` files under `cold/`, each with its own bytes.
fn cold_rig(label: &str) -> Rig {
    Rig::built(label, |source| {
        fs::create_dir(source.join("cold")).unwrap();
        for n in 0..COLD {
            let path = source.join(format!("cold/f-{n:02}"));
            fs::write(&path, pattern(n as u8, COLD_BYTES)).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        stamp_tree(source);
    })
}
/// `count` cold readers parked behind a fully leased read set, then Force.
/// Each is parked in its first READ, after an OPEN that needed no reader.
/// With more readers than handoff slots the mount is saturated: sixteen
/// READs admitted and parked, the receive unit held by a callback waiting
/// for a slot, and the rest queued in the kernel.
///
/// The staging does not fix how the teardown ends. The kernel's abort returns
/// every reader's call, but a reader that has not yet dropped the path
/// reference of that call when the product makes its one detach keeps the
/// mount busy, and the teardown then stops `Retained` at `Detach`. Both ends
/// are checked and the one observed is printed.
fn parked_readers_are_ended(name: &'static str, label: &str, count: usize) {
    fusectl::mount();
    let rig = cold_rig(label);
    let ready = rig.mount(1);
    let _guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let mut checks = Checks::default();
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    let mount = root(&ready).to_owned();
    // The names are looked up before the hold, so each reader's OPEN is
    // decided by one owner visit from what the daemon remembers and
    // completes. Its one request in flight is then its first READ, which
    // needs base bytes and so a Store reader.
    passed(
        &bash_in(COMMAND, &mount, "stat cold/* > /dev/null"),
        "before the hold",
    );
    quiet(&rig, token);
    let admitted = count.min(HANDOFFS);
    let received = (count - admitted).min(RECEIVE_UNITS);
    // Declared before the leases: on any path the leases are returned first,
    // so a process that is still parked can finish and be reaped.
    let mut readers: Vec<Proc> = Vec::new();
    let leases = holds::Leases::all(&rig.store, WAIT);
    let (held, idle) = (rig.store.read_work(), work(&rig, token));
    readers.extend((0..count).map(|n| {
        Proc::python(
            format!("cold reader {n}"),
            READER,
            &mount.join(format!("cold/f-{n:02}")),
        )
    }));
    // Readiness of the staging: every reader is inside its call once the
    // kernel holds one request of this connection for each of them. A reader
    // that had not reached the mount yet would later open a path that no
    // longer exists, which is not an answer of the aborted connection.
    within("the readers' requests parked", || {
        let now = work(&rig, token);
        (now.admitted, now.parked, now.received)
            == (admitted as u32, admitted as u32, received as u32)
            && connection.waiting() == count as u64
    });
    let before = work(&rig, token);
    let waiting = rig.store.read_work();
    assert_eq!(
        (waiting.leased, waiting.waiting, waiting.grants),
        (leases.count(), admitted, held.grants),
        "every admitted request waits for a reader: {waiting:?}"
    );
    // Every handed-off request is a parked READ or a completed OPEN: the
    // OPEN of every reader that is parked, and of no more than every reader.
    let opened = before.completed - idle.completed;
    assert_eq!(
        before.handoffs - idle.handoffs,
        admitted as u64 + opened,
        "every handed-off request is parked or completed: {before:?}"
    );
    assert!(
        (admitted as u64..=count as u64).contains(&opened),
        "completed OPENs: {opened}"
    );
    let blocked = readers
        .iter_mut()
        .map(|reader| usize::from(reader.running()))
        .sum::<usize>();
    assert_eq!(blocked, count, "every reader is blocked in its call");
    println!(
        "{name} before: readers={count} leases_held={} admitted={} parked={} received={} terminal={} read_admissions_waiting={} kernel_waiting={} readers_blocked={blocked}",
        leases.count(),
        before.admitted,
        before.parked,
        before.received,
        before.terminal,
        waiting.waiting,
        connection.waiting()
    );

    let ended = force(&rig.harness, token);
    // Force returned while the test still holds every reader of the Store:
    // no reader was granted to anyone and every waiting admission is gone.
    let after = rig.store.read_work();
    checks.that(
        (after.leased, after.waiting, after.grants) == (leases.count(), 0, held.grants),
        || format!("{name}: the read set after Force, leases still held: {after:?}"),
    );
    // Every reader's call returned an error; each exits by its own status.
    let mut answers = BTreeMap::<String, usize>::new();
    for (n, reader) in readers.iter_mut().enumerate() {
        let answer = match reader.wait(WAIT) {
            Some((status, output)) => {
                let text = String::from_utf8_lossy(&output).trim().to_owned();
                let failed = errno(&text).is_some_and(|errno| ABORTED.contains(&errno));
                checks.that(
                    status.code() == Some(7)
                        && status.signal().is_none()
                        && failed
                        && text.ends_with("signals 0"),
                    || format!("{name}: cold reader {n} ended {status:?} with {text:?}"),
                );
                text
            }
            None => {
                checks.that(false, || {
                    format!("{name}: cold reader {n} was still blocked {WAIT:?} after Force")
                });
                "still blocked".to_owned()
            }
        };
        *answers.entry(answer).or_default() += 1;
    }
    let still = rig.store.read_work();
    println!(
        "{name} readers: answers={answers:?} leases_still_held={} read_admissions_waiting={} reader_grants={}->{}",
        still.leased, still.waiting, held.grants, still.grants
    );

    let (facts, last, done) = match ended {
        Ended::Unmounted(outcome, done) => {
            println!("{name} outcome={outcome:?}");
            println!("{name} receipt={}", brief(&done.native));
            checks.that(
                outcome.facts.abort == AbortDisposition::Written
                    && outcome.facts.detach == DetachDisposition::Detached
                    && outcome.facts.commit == CommitKnowledge::Absent
                    && done.completion.is_some(),
                || format!("{name}: {outcome:?}"),
            );
            assert!(mount_entry(&ready.directory).is_none());
            assert!(!mount.exists());
            within("own connection directory removed", || !connection.exists());
            for request in [Request::Status(token), Request::Unmount(token)] {
                assert!(
                    matches!(
                        rig.harness.service.execute_control(&request),
                        Err(Failure::Rejected(ControlCode::Missing, _))
                    ),
                    "{request:?}"
                );
            }
            (outcome.facts.clone(), outcome.work, Some(done))
        }
        // The kernel kept the aborted mount for a reference a caller had not
        // dropped yet when the one detach was made: the truthful stage.
        Ended::Retained(custody) => {
            println!("{name} custody={custody:?}");
            checks.that(aborted_and_mounted(&custody, token), || {
                format!("{name}: retained, but not aborted and mounted at Detach: {custody:?}")
            });
            (
                custody.forced.clone().expect("forced facts"),
                custody.work.expect("connection counters"),
                None,
            )
        }
    };
    // One terminal reply for each admitted request that was fenced before
    // its attempt, one error attempt for each callback that held a receive
    // unit, and nothing left or retained.
    checks.that(
        facts.fenced == admitted as u64
            && last.terminal - before.terminal == received as u64
            && (last.received, last.admitted, last.retained) == (0, 0, 0),
        || {
            format!(
                "{name}: fenced={} (admitted {admitted}), terminal {} -> {} (received {received}), left {:?}",
                facts.fenced, before.terminal, last.terminal, last
            )
        },
    );
    println!(
        "{name} force: disposition={} abort={:?} detach={:?} fenced={} admitted_before={admitted} terminal={}->{} received_before={received} left=({},{},{}) completed={}->{} leases_held_through_force={}",
        if done.is_some() { "ForceUnmounted" } else { "Retained" },
        facts.abort,
        facts.detach,
        facts.fenced,
        before.terminal,
        last.terminal,
        last.received,
        last.admitted,
        last.retained,
        before.completed,
        last.completed,
        leases.count()
    );

    leases.release();
    let released = rig.store.read_work();
    assert_eq!(
        (released.leased, released.waiting, released.quarantined),
        (0, 0, 0),
        "{released:?}"
    );
    drop(readers);
    match done {
        Some(done) => {
            drop(done);
            rig.finish();
        }
        None => {
            let detached = plain_umount(&ready.directory);
            assert!(detached.success(), "{name}: umount: {detached:?}");
            println!(
                "{name} teardown: by_test=plain umount(8) status={:?}",
                detached.code()
            );
            abandon(rig);
        }
    }
    checks.done(name);
}

/// FP-19-FS, the unattempted half: one cold reader parked before its demand.
#[test]
fn fp19_force_releases_a_parked_cold_reader_while_the_read_set_stays_leased() {
    parked_readers_are_ended("FP-19-FS", "fp19", 1);
}

/// What a terminal unmount accounts a READ by. A READ makes one owner visit
/// that records nothing and may then wait for a Store reader. Here it is
/// parked on reader admission behind a fully leased read set, through a
/// descriptor an external process opened before the hold. The engine's
/// maintained row counts are what they were with the descriptor idle (no
/// source, reader or lease row of the request) and it counts no base
/// reader. Normal Unmount is refused by the kernel's own reversible answer
/// and changes nothing. Force then ends the request through the mount's
/// fence: one terminal reply, nothing received, admitted or retained when
/// the one detach is made, and no reader granted. The process keeps its
/// descriptor, so the kernel keeps the aborted mount and the teardown stops
/// `Retained` at `Detach`, as in FP-33.
#[test]
fn a_read_parked_after_its_visit_holds_no_engine_row_and_is_ended_by_the_fence() {
    let name = "PARKED-READ";
    fusectl::mount();
    let rig = Rig::new("parked-read");
    let helper = rig.harness.bind(1);
    let ready = rig.mount(2);
    let _guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let mut checks = Checks::default();
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let mount = root(&ready).to_owned();
    // The descriptor is opened and read once while readers are granted.
    let mut holder = Holder::spawn("descriptor", &mount);
    let served = holder.ask("probe");
    assert!(
        served.starts_with("ok "),
        "{name}: before the hold: {served}"
    );
    quiet(&rig, token);
    let idle_rows = rig.harness.engine(helper);
    let leases = holds::Leases::all(&rig.store, WAIT);
    let (held, idle) = (rig.store.read_work(), work(&rig, token));

    // One READ through the held descriptor: its visit, then the reader wait.
    writeln!(holder.input.as_mut().unwrap(), "probe").unwrap();
    within("the READ is parked on reader admission", || {
        let now = work(&rig, token);
        (now.admitted, now.parked, now.received) == (1, 1, 0) && rig.store.read_work().waiting == 1
    });
    let before = work(&rig, token);
    assert_eq!(before.handoffs - idle.handoffs, 1, "one parked request");
    let parked_rows = rig.harness.engine(helper);
    let local = rig
        .harness
        .status(token)
        .local
        .expect("the engine is observed beside the parked READ");
    println!(
        "{name} parked: admitted={} parked={} read_admissions_waiting=1 engine_rows(idle={idle_rows:?} parked={parked_rows:?}) base_readers={}",
        before.admitted, before.parked, local.base_readers
    );
    checks.that(parked_rows == idle_rows && local.base_readers == 0, || {
        format!(
            "{name}: the parked READ holds engine rows: {idle_rows:?} -> {parked_rows:?}, base_readers={}",
            local.base_readers
        )
    });

    // Normal Unmount: the kernel counts the caller blocked in its READ.
    match rig.harness.try_unmount(token) {
        Err(Failure::Native(failure)) => checks.that(
            (failure.code, failure.phase) == (ControlCode::Busy, "unmount:kernel"),
            || format!("{name}: Unmount beside the parked READ: {failure:?}"),
        ),
        other => panic!("{name}: Unmount beside the parked READ: {}", brief(&other)),
    }
    let busy = work(&rig, token);
    checks.that(
        rig.harness.phase(token) == NativePhase::Ready
            && (busy.admitted, busy.parked, busy.terminal) == (1, 1, before.terminal)
            && rig.store.read_work().waiting == 1,
        || format!("{name}: the busy Unmount had an effect: {before:?} -> {busy:?}"),
    );

    let custody = match force(&rig.harness, token) {
        Ended::Retained(custody) => *custody,
        Ended::Unmounted(outcome, _) => {
            panic!("{name}: forced past a held descriptor: {outcome:?}")
        }
    };
    println!("{name} custody={custody:?}");
    checks.that(aborted_and_mounted(&custody, token), || {
        format!("{name}: not Retained at Detach with abort Written and detach Busy: {custody:?}")
    });
    // The fence ended the one admitted request before its demand, and the
    // local drain held before the one detach.
    checks.that(
        custody
            .forced
            .as_ref()
            .is_some_and(|facts| facts.fenced == 1)
            && custody.work.is_some_and(|work| {
                (work.received, work.admitted, work.retained) == (0, 0, 0)
                    && work.loops_joined == work.loops_configured
                    && work.loops_configured != 0
            }),
        || {
            format!(
                "{name}: the parked READ was not ended by the fence before the detach: {:?} {:?}",
                custody.forced, custody.work
            )
        },
    );
    let after = rig.store.read_work();
    checks.that(
        (after.leased, after.waiting, after.grants) == (leases.count(), 0, held.grants),
        || format!("{name}: the read set after Force, leases still held: {after:?}"),
    );
    // The blocked caller got the kernel's answer for an aborted connection.
    let answer = holder.line();
    checks.that(
        errno(&answer).is_some_and(|errno| ABORTED.contains(&errno)),
        || format!("{name}: the parked READ answered {answer:?}, not one of {ABORTED:?}"),
    );
    println!(
        "{name} force: fenced={:?} left={:?} reader_grants={}->{} read_admissions_waiting={} read_answer={answer:?}",
        custody.forced.as_ref().map(|facts| facts.fenced),
        custody
            .work
            .map(|work| (work.received, work.admitted, work.retained)),
        held.grants,
        after.grants,
        after.waiting
    );

    leases.release();
    let (signals, exit) = holder.leave();
    checks.that(
        exit.code() == Some(0) && exit.signal().is_none() && signals == "signals 0",
        || format!("{name}: the holder ended {exit:?} after {signals:?}"),
    );
    let detached = plain_umount(&ready.directory);
    assert!(detached.success(), "{name}: umount: {detached:?}");
    assert!(mount_entry(&ready.directory).is_none());
    abandon(rig);
    checks.done(name);
}

/// FP-34, terminal half: twenty cold readers saturate the mount.
#[test]
fn fp34_force_ends_every_admitted_and_received_callback_of_a_saturated_mount() {
    parked_readers_are_ended("FP-34 terminal", "fp34-terminal", COLD);
}

/// FP-22-FS, in this container's single mount namespace: a reference in A
/// gives the truthful normal Busy on A and never holds sibling B; Force on A
/// leaves A aborted and mounted, the holder alive and B serving.
#[test]
fn fp22_a_reference_in_a_never_holds_b_and_force_on_a_signals_no_process() {
    let name = "FP-22-FS";
    fusectl::mount();
    let rig = Rig::new("fp22");
    let a = rig.mount(1);
    let b = rig.mount(2);
    let _guards = [Mounted(a.directory.clone()), Mounted(b.directory.clone())];
    let mut checks = Checks::default();
    assert!(a.receipt.abort_bound && b.receipt.abort_bound);
    let (on_a, on_b) = (fusectl::own(&a), fusectl::own(&b));
    assert_ne!(on_a.minor, on_b.minor, "two connections");
    for (ready, bytes) in [(&a, "a"), (&b, "b")] {
        passed(
            &bash_in(
                COMMAND,
                root(ready),
                &format!("printf '{bytes}\\n' > own.txt"),
            ),
            "before the holder",
        );
    }
    let mut holder = Holder::spawn("cwd", root(&a));
    assert_eq!(holder.ask("probe"), "errno 2 ENOENT");
    quiet(&rig, a.token);
    quiet(&rig, b.token);
    let a_row = row(&a.directory).expect("A's mount row");
    let b_row = row(&b.directory).expect("B's mount row");

    // Normal Unmount of A: the kernel's reversible answer, nothing else.
    let busy = match rig.harness.try_unmount(a.token) {
        Err(Failure::Native(refusal)) => (refusal.code, refusal.phase),
        other => panic!("{name}: Unmount of A beside its holder: {}", brief(&other)),
    };
    let probed = rig.harness.status(a.token);
    let served = holder.ask("probe");
    checks.that(
        busy == (ControlCode::Busy, "unmount:kernel")
            && probed.native.as_ref().map(|native| native.phase) == Some(NativePhase::Ready)
            && probed.activity == Activity::Idle
            && served == "errno 2 ENOENT",
        || {
            format!(
                "{name}: normal Unmount of A: {busy:?}, then phase {:?}, activity {:?}, holder call {served:?}",
                probed.native.as_ref().map(|native| native.phase),
                probed.activity
            )
        },
    );
    println!(
        "{name} normal_unmount_of_a={busy:?} a_phase_after={:?} holder_call={served:?}",
        probed.native.as_ref().map(|native| native.phase)
    );

    let b_before = work(&rig, b.token);
    let custody = match force(&rig.harness, a.token) {
        Ended::Retained(custody) => *custody,
        Ended::Unmounted(outcome, _) => panic!("{name}: forced past A's holder: {outcome:?}"),
    };
    println!("{name} custody_of_a={custody:?}");
    checks.that(aborted_and_mounted(&custody, a.token), || {
        format!("{name}: A is not aborted and mounted at Detach: {custody:?}")
    });
    let alive = holder.alive();
    let refused = holder.ask("probe");
    checks.that(
        alive && errno(&refused).is_some_and(|errno| ABORTED.contains(&errno)),
        || format!("{name}: the holder after Force on A: alive={alive}, call {refused:?}"),
    );
    checks.that(row(&a.directory).as_deref() == Some(a_row.as_str()), || {
        format!("{name}: A's mount row changed")
    });

    // B is another connection: untouched by A's abort, still serving an
    // ordinary process, and its normal unmount succeeds beside A's holder.
    let mut writer = Proc::bash(
        "write on B after Force on A",
        root(&b),
        "set -eu; printf 'after\\n' >> own.txt; cat own.txt",
    );
    let wrote = writer.wait(WAIT);
    let b_after = work(&rig, b.token);
    let b_status = rig.harness.status(b.token);
    checks.that(
        wrote
            .as_ref()
            .is_some_and(|(status, output)| status.success() && output == b"b\nafter\n")
            && b_after.completed > b_before.completed
            && b_after.terminal == 0
            && b_status.native.as_ref().map(|native| native.phase) == Some(NativePhase::Ready)
            && row(&b.directory).as_deref() == Some(b_row.as_str())
            && on_b.exists(),
        || {
            format!(
                "{name}: B after Force on A: write {wrote:?}, {b_before:?} -> {b_after:?}, phase {:?}",
                b_status.native.as_ref().map(|native| native.phase)
            )
        },
    );
    assert_eq!(fs::read(root(&b).join("own.txt")).unwrap(), b"b\nafter\n");
    println!(
        "{name} after_force_on_a: a_stage={:?} a_detach={:?} a_mount_row=unchanged holder_alive={alive} holder_call={refused:?} b_write={:?} b_completed={}->{} b_terminal={} b_phase={:?} b_own_connection={}",
        custody.stage,
        custody.forced.as_ref().map(|facts| facts.detach),
        wrote.as_ref().map(|(status, _)| status.code()),
        b_before.completed,
        b_after.completed,
        b_after.terminal,
        b_status.native.as_ref().map(|native| native.phase),
        on_b.exists()
    );
    drop(writer);
    let receipt = rig.unmount(&b);
    let still = holder.alive();
    checks.that(
        receipt.contains("Drained")
            && still
            && row(&a.directory).as_deref() == Some(a_row.as_str()),
        || format!("{name}: after B's unmount: holder alive={still}, {receipt}"),
    );
    println!(
        "{name} normal_unmount_of_b=Unmounted drained={} holder_alive={still} a_mount_row=unchanged:{}",
        receipt.contains("Drained"),
        row(&a.directory).as_deref() == Some(a_row.as_str())
    );

    let (signals, exit) = holder.leave();
    checks.that(
        exit.code() == Some(0) && exit.signal().is_none() && signals == "signals 0",
        || format!("{name}: the holder ended {exit:?} after {signals:?}"),
    );
    let detached = plain_umount(&a.directory);
    assert!(detached.success(), "{name}: umount: {detached:?}");
    println!(
        "{name} teardown: holder_exit={:?} {signals:?} by_test=plain umount(8) of A status={:?} scope=single mount namespace of the test container deviations={}",
        exit.code(),
        detached.code(),
        checks.0.len()
    );
    abandon(rig);
    checks.done(name);
}
