//! Real kernel mount: a Commit that has published and is about to install
//! returns when descriptors are open on files that were created since the
//! previous Commit and unlinked after its capture, and the Workspace keeps
//! serving.
//!
//! Staging, hook-free. An external process that nothing registered opens the
//! files. The product control Commit runs over `support/history_gate.rs`, which
//! holds its thread after the real publication and before the local install,
//! while the Commit owns its captured reader. The files are then unlinked, so
//! each orphan waits for the captured generation, and the Commit is released.
//! The holder, the unlinker and the Commit are three independent parties.
//!
//! Bounds. From Ready to the unmount, no step waits on the mount or on the
//! Workspace's lock on the test thread without a limit. Every external script
//! is the leader of its own process group and enters the mount itself (a
//! `current_dir` would make `spawn` wait for the mount); every owner or control
//! call runs on a thread whose answer is awaited on a channel; both Commits
//! run on their own threads and are only observed. What does not answer in
//! time ends the experiment (`Scene::abandon`): the test tries to detach its
//! own mount lazily, bounded, kills its process groups and waits for them
//! bounded, prints what it saw and fails. It never joins, retries or helps the
//! stuck Commit, releases no reader early and touches no other FUSE connection
//! (nothing is written under `/sys/fs/fuse/connections`, `support/fusectl.rs`).
//! Bringing the Workspace up and stopping it cleanly are the harness's own.
//!
//! That cleanup is not complete, and cannot be: the stuck Commit's thread, the
//! harness with its serial gate and the fixture directory stay behind, a
//! process blocked in the kernel on the mount cannot be reaped before its
//! connection ends, and a mount that could not be detached stays in the mount
//! table. They belong to this process, whose exit closes its own connection.
//! So this test is the only one in its binary.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/history_gate.rs"]
mod history_gate;
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
use history_gate::Point;
use layerfs_bridge::control::{CommitControl, Reply, Request, UnmountControl, WorkspaceToken};
use layerfs_daemon::{
    control::Service,
    store::{ReaderFactory, ReleasedOwner, Store, StoreReader},
};
use layerfs_history::CommitOutcome;
use layerfs_persistence::Handles;
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage};
use model::Model;
use mounted::{mount_entry, Harness, COMMAND};
use nix::libc;
use rig::{base_tree, root};
use std::{
    cell::Cell,
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// The wait for the Commit and for each step and call of the experiment.
const WAIT: Duration = Duration::from_secs(5);
/// The wait for one probe or script or for the lazy unmount, and `REAP` for the
/// reaping of a killed process group's leader.
const PROBE: Duration = Duration::from_secs(3);
const REAP: Duration = Duration::from_secs(1);
/// How long the held Commit thread stays held if nothing releases it.
const LIMIT: Duration = Duration::from_secs(20);
/// The files the holder keeps open: created since the last Commit.
const MAKE: &str =
    "set -eu; mkdir held; for n in 1 2 3; do printf 'file %s\\n' $n > held/f$n; done";
/// Opens a descriptor on each, says so, holds them until standard input
/// closes and then reads each one: the bytes survive the unlink and the Commit.
const HOLD: &str = r#"
set -eu
exec 3<>held/f1 4<>held/f2 5<>held/f3
echo opened
read -r _ || true
IFS= read -r one <&3
IFS= read -r two <&4
IFS= read -r three <&5
[ "$one" = "file 1" ] && [ "$two" = "file 2" ] && [ "$three" = "file 3" ]
"#;

/// The condition within `wait`. The condition must not block: it is not
/// interrupted.
fn within(wait: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + wait;
    while !condition() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(2));
    }
    true
}
/// The call on a thread of its own, awaited for `wait`. A call that does not
/// answer leaves its thread behind.
fn bounded<T: Send + 'static>(
    wait: Duration,
    call: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (send, receive) = mpsc::channel();
    let thread = thread::spawn(move || {
        let _ = send.send(call());
    });
    match receive.recv_timeout(wait) {
        Ok(value) => {
            let _ = thread.join();
            Ok(value)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!("no answer within {wait:?}")),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("the call panicked".into()),
    }
}
/// The condition within `wait`, each evaluation a bounded call of its own.
fn eventually(
    wait: Duration,
    what: &str,
    condition: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<(), String> {
    let condition = Arc::new(condition);
    let deadline = Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(format!("{what}: not within {wait:?}"));
        }
        let once = condition.clone();
        match bounded(left, move || once()) {
            Ok(true) => return Ok(()),
            Ok(false) => thread::sleep(Duration::from_millis(2)),
            Err(why) => return Err(format!("{what}: {why}")),
        }
    }
}

/// An external process, the leader of its own process group so that what it
/// starts is killed with it.
struct Spawned {
    child: Child,
    ended: bool,
}
impl Spawned {
    fn start(mut command: Command) -> Self {
        Self {
            child: command.process_group(0).spawn().unwrap(),
            ended: false,
        }
    }
    /// The exit status, or `None` if it has not ended within `wait`.
    fn finish(&mut self, wait: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + wait;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.ended = true;
                    return Some(status);
                }
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                _ => return None,
            }
        }
    }
    fn signal_group(&self) {
        // SAFETY: only signals the group this process leads, while it is unreaped.
        unsafe { libc::killpg(self.child.id() as i32, libc::SIGKILL) };
    }
    /// Kills the whole group and waits `REAP` for its leader. False: the
    /// leader is blocked in the kernel on the mount, and ends with the
    /// connection.
    fn kill(&mut self) -> bool {
        if !self.ended {
            self.signal_group();
        }
        self.ended || self.finish(REAP).is_some()
    }
}
impl Drop for Spawned {
    fn drop(&mut self) {
        if !self.ended {
            self.signal_group();
        }
    }
}
/// A `bash -c` of the command identity that enters the mount itself.
fn script(mount: &Path, text: &str, input: Stdio, output: Stdio) -> Spawned {
    let mut command = Command::new("bash");
    command
        .args([
            "-c",
            &format!("cd '{}' || exit 97\n{text}", mount.display()),
        ])
        .uid(COMMAND)
        .gid(COMMAND)
        .env("LC_ALL", "C")
        .stdin(input)
        .stdout(output)
        .stderr(Stdio::null());
    Spawned::start(command)
}
/// A script that must end with status 0 within `PROBE`.
fn succeeds(mount: &Path, what: &str, text: &str) -> Result<(), String> {
    let mut child = script(mount, text, Stdio::null(), Stdio::null());
    match child.finish(PROBE) {
        Some(status) if status.success() => Ok(()),
        Some(status) => Err(format!("{what}: {status}")),
        None => Err(format!(
            "{what}: no answer within {PROBE:?} (killed; leader reaped: {})",
            child.kill()
        )),
    }
}
/// A readdir and a create of two ordinary processes at once: what a blocked
/// Workspace does not answer.
fn serves(mount: &Path, name: &str) -> Result<(), String> {
    let mut probes = [
        (
            "readdir",
            script(mount, "ls held > /dev/null", Stdio::null(), Stdio::null()),
        ),
        (
            "create",
            script(mount, &format!(": > {name}"), Stdio::null(), Stdio::null()),
        ),
    ];
    let deadline = Instant::now() + PROBE;
    let mut failures = Vec::new();
    for (what, probe) in &mut probes {
        match probe.finish(deadline.saturating_duration_since(Instant::now())) {
            Some(status) if status.success() => {}
            Some(status) => failures.push(format!("{what}: {status}")),
            None => failures.push(format!(
                "{what}: no answer (killed; leader reaped: {})",
                probe.kill()
            )),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// The test's own mount, detached lazily at most once, bounded like every
/// other external process. Whether the mount entry is gone is printed.
struct Mounted {
    directory: String,
    tried: Cell<bool>,
}
impl Mounted {
    fn detach(&self) {
        if self.tried.replace(true) || mount_entry(&self.directory).is_none() {
            return;
        }
        let mut command = Command::new("umount");
        command
            .args(["-l", &self.directory])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut umount = Spawned::start(command);
        let status = umount.finish(PROBE);
        if status.is_none() {
            umount.kill();
        }
        println!(
            "ORPHAN_COMMIT_GUARD lazy umount of {} by the test: {status:?}; mount entry {}",
            self.directory,
            if mount_entry(&self.directory).is_none() {
                "gone"
            } else {
                "still there"
            }
        );
    }
}
impl Drop for Mounted {
    fn drop(&mut self) {
        self.detach();
    }
}

/// One installed Store whose first publication is held after it happened:
/// the same public pieces `open_store` composes, with the History port wrapped.
fn gated(label: &str) -> (installed::Fixture, Arc<Store>, history_gate::Hold) {
    let fixture = installed::Fixture::built(label, |source| {
        base_tree(source);
        Model::host(source).1
    });
    let handles = Arc::new(
        Handles::open_writable(
            fixture.config.clone(),
            installed::BINDING,
            installed::CURSOR,
        )
        .unwrap(),
    );
    let writer: Arc<dyn PackPersistence> = handles.storage.clone();
    let (catalog, hold) = history_gate::gated(handles, Point::BeforeInstall, LIMIT);
    let reader_config = fixture.config.clone();
    let factory: ReaderFactory = Arc::new(move |_| {
        let read =
            Handles::open_read_only(reader_config.clone(), installed::BINDING, installed::CURSOR)
                .unwrap();
        Ok(StoreReader::new(
            Storage::new(read.storage).unwrap(),
            Arc::new(read.history),
        ))
    });
    let control = factory(0).unwrap();
    let store = Arc::new(
        Store::new(
            writer,
            Arc::new(catalog),
            control,
            factory,
            2 * 1024 * 1024,
            ReservationBlocks::default(),
            16,
        )
        .unwrap(),
    );
    (fixture, store, hold)
}

/// What the Commit thread reports: its outcome, and whether the constructor's
/// reader and operation owner were released in that order with nothing kept.
struct Finished {
    outcome: Result<CommitOutcome, String>,
    released: bool,
}
impl Finished {
    fn committed(&self, which: &str) -> Result<(), String> {
        match (&self.outcome, self.released) {
            (Ok(CommitOutcome::Committed { .. }), true) => Ok(()),
            (Ok(CommitOutcome::Committed { .. }), false) => Err(format!(
                "the {which} Commit did not release its reader, then its owner"
            )),
            (other, _) => Err(format!("the {which} Commit ended {other:?}")),
        }
    }
}
/// One product control Commit on its own thread, attempted once.
fn commit(service: Arc<Service>, token: WorkspaceToken) -> JoinHandle<Finished> {
    thread::spawn(move || {
        let request = Request::Commit(CommitControl {
            token,
            force: false,
            message: None,
        });
        match service.execute_control(&request) {
            Ok(done) => Finished {
                outcome: match &done.reply {
                    Reply::Committed(outcome) => Ok(outcome.clone()),
                    other => Err(format!("{other:?}")),
                },
                released: done
                    .commit
                    .as_ref()
                    .and_then(|commit| commit.namespace.as_ref())
                    .is_some_and(|namespace| {
                        !namespace.retained()
                            && namespace.released
                                == [ReleasedOwner::Reader, ReleasedOwner::Operation]
                            && namespace.release_error.is_none()
                    }),
            },
            Err(failure) => Finished {
                outcome: Err(format!("{failure:?}").chars().take(400).collect()),
                released: false,
            },
        }
    })
}
/// The Workspace's generation waits, open orphans and maintenance targets, read
/// as an owner job.
fn counts(service: &Service, token: WorkspaceToken) -> Option<(u64, u64, u64)> {
    let resources = service.operation(token).ok()?.overlay().resources().ok()?;
    Some((
        resources.open.generation_waits,
        resources.open.orphan_files,
        resources.counts.maintenance_targets,
    ))
}

/// The Store parts that only a clean end drops.
struct Parts {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
}
/// Everything of the experiment, so that every way out of it is bounded.
struct Scene {
    parts: Option<Parts>,
    hold: history_gate::Hold,
    guard: Mounted,
    mount: PathBuf,
    token: WorkspaceToken,
    holder: Option<Spawned>,
    commit: Option<JoinHandle<Finished>>,
}
impl Scene {
    fn up() -> Self {
        let (fixture, store, hold) = gated("orphan-commit");
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        let ready = harness.mount(1);
        Self {
            guard: Mounted {
                directory: ready.directory.clone(),
                tried: Cell::new(false),
            },
            mount: root(&ready).to_path_buf(),
            token: ready.token,
            hold,
            holder: None,
            commit: None,
            parts: Some(Parts {
                fixture,
                store,
                harness,
            }),
        }
    }
    fn service(&self) -> Arc<Service> {
        self.parts.as_ref().unwrap().harness.service.clone()
    }
    /// The counts satisfy `holds`, within `WAIT`.
    fn counted(&self, what: &str, holds: fn((u64, u64, u64)) -> bool) -> Result<(), String> {
        let (service, token) = (self.service(), self.token);
        eventually(WAIT, what, move || {
            counts(&service, token).is_some_and(holds)
        })
    }
    /// The Commit's end, observed for `WAIT`. A Commit that does not end is
    /// kept, never joined.
    fn settled(&mut self, running: JoinHandle<Finished>, which: &str) -> Result<Finished, String> {
        if within(WAIT, || running.is_finished()) {
            return running
                .join()
                .map_err(|_| "the Commit thread panicked".into());
        }
        self.commit = Some(running);
        Err(format!(
            "the {which} Commit did not return within {WAIT:?}; the Workspace answers: {:?}",
            serves(&self.mount, "served-during-the-stall")
        ))
    }
    fn experiment(&mut self) -> Result<(), String> {
        // The files exist in the Workspace only: made since the last Commit.
        succeeds(&self.mount, "make the files", MAKE)?;
        let mut holder = script(&self.mount, HOLD, Stdio::piped(), Stdio::piped());
        let mut said = BufReader::new(holder.child.stdout.take().unwrap());
        self.holder = Some(holder);
        let line = bounded(WAIT, move || {
            let mut line = String::new();
            said.read_line(&mut line).map(|_| line)
        })?
        .map_err(|error| error.to_string())?;
        if line.trim() != "opened" {
            return Err(format!(
                "the holder said {line:?}, not that it holds the files"
            ));
        }

        // The Commit publishes and is held before its local install.
        let running = commit(self.service(), self.token);
        if !self.hold.entered(WAIT) {
            self.commit = Some(running);
            return Err("the Commit did not reach its hold before the install".into());
        }
        // Unlinked after the capture: each orphan waits for the captured generation.
        succeeds(
            &self.mount,
            "unlink the files",
            "rm held/f1 held/f2 held/f3",
        )?;
        self.counted("the orphans wait for the captured generation", |c| c.0 == 3)?;
        self.hold.release();
        self.settled(running, "first")?.committed("first")?;

        // The Workspace serves, the orphans were woken by the release of their
        // generation, and the descriptors still read the unlinked files.
        serves(&self.mount, "served-after-the-commit")?;
        self.counted("the orphans left the generation", |c| c.0 == 0)?;
        let mut holder = self.holder.take().unwrap();
        drop(holder.child.stdin.take());
        let read = holder.finish(WAIT);
        if !read.is_some_and(|status| status.success()) {
            self.holder = Some(holder);
            return Err(format!(
                "the descriptors did not read their files: {read:?}"
            ));
        }
        self.counted("the orphans are reclaimed", |c| c.1 == 0 && c.2 == 0)?;
        // What was unlinked after the capture is the next Commit's, observed like the first.
        let next = commit(self.service(), self.token);
        self.settled(next, "next")?.committed("next")?;

        let (service, token) = (self.service(), self.token);
        eventually(WAIT, "the connection is quiet", move || {
            matches!(
                service.execute_control(&Request::Status(token)),
                Ok(done) if matches!(&done.reply, Reply::Status(status)
                    if status.kernel.as_ref().and_then(|kernel| kernel.work)
                        .is_some_and(|work| work.active == 0))
            )
        })?;
        let (service, token) = (self.service(), self.token);
        let (unmounted, receipt) = bounded(2 * WAIT, move || {
            let request = Request::Unmount(UnmountControl {
                token,
                discard: true,
            });
            match service.execute_control(&request) {
                Ok(done) => Ok((
                    done.reply == Reply::Unmounted(token),
                    format!("{:?}", done.kernel),
                )),
                Err(failure) => Err(format!("{failure:?}")),
            }
        })??;
        if !unmounted || !receipt.contains("Drained") {
            return Err(format!("the unmount did not drain: {receipt}"));
        }
        let directory = &self.guard.directory;
        if mount_entry(directory).is_some() || Path::new(directory).exists() {
            return Err("the mount is still there after the unmount".into());
        }
        Ok(())
    }
    /// The end of a clean run.
    fn down(mut self) {
        let Parts {
            fixture,
            store,
            harness,
        } = self.parts.take().unwrap();
        harness.stop();
        drop(store);
        fixture.cleanup();
    }
    /// The end of a run that did not answer or did not hold. Detached first, so
    /// that nothing here waits on the mount; every wait after is bounded. The
    /// stuck Commit's thread, the harness, its serial gate and the fixture
    /// directory are left to the process.
    fn abandon(mut self, why: &str) -> ! {
        self.guard.detach();
        if let Some(mut holder) = self.holder.take() {
            println!(
                "ORPHAN_COMMIT_GUARD holder killed; leader reaped: {}",
                holder.kill()
            );
        }
        println!(
            "ORPHAN_COMMIT_GUARD left to the process: the harness, its gate, the stuck Commit's thread and {}",
            self.parts.as_ref().unwrap().fixture.directory.display()
        );
        panic!("{why}");
    }
}
impl Drop for Scene {
    fn drop(&mut self) {
        // Held parts are reached here only when the experiment ended badly: they
        // are never dropped, since dropping them waits on the Workspace.
        if let Some(parts) = self.parts.take() {
            std::mem::forget(parts);
        }
    }
}

#[test]
fn a_commit_returns_when_descriptors_are_open_on_files_unlinked_after_its_capture() {
    let mut scene = Scene::up();
    if let Err(why) = scene.experiment() {
        scene.abandon(&why);
    }
    scene.down();
}
