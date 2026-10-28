//! A normal unmount whose connection outlives its mount (issues #426, #460).
//!
//! Real kernel mount, staging, hook-free. After Attach, `unshare -m` makes a
//! second mount namespace that holds a copy of the Workspace mount, the way an
//! engine or VM agent outside a Sandbox can. The daemon's plain detach then
//! succeeds in its own namespace while the superblock lives on in the copy, so
//! the kernel never ends the connection: the receive loops stay blocked and
//! the drain stops at Join. The normal unmount then makes the same single
//! abort write a forced unmount would, drains again, revokes and closes, so it
//! ends the connection itself. Removing the mount directory then detaches the
//! copy's mount too (Linux detaches the mounts on a removed mount point in
//! every namespace), so the connection's control entry goes while the copy
//! still runs, and the copy finds no mount there.
//!
//! The safety rule of `support/fusectl.rs` binds this file: nothing here
//! writes under `/sys/fs/fuse/connections` or reads another connection.
#![cfg(target_os = "linux")]
#[path = "support/abort_write.rs"]
mod abort_write;
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
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
    AbortDisposition, CommitKnowledge, DetachDisposition, Reply, Request, TeardownStage,
    UnmountControl,
};
use layerfs_daemon::control::Failure;
use mounted::{mount_entry, until};
use rig::{root, Rig};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the test.
const WAIT: Duration = Duration::from_secs(5);

/// Runs in its own private copy of the mount namespace, so it holds a copy of
/// the Workspace mount without touching it. It answers one line for each line
/// it is sent: `probe` looks up a name never looked up before under the
/// copied mount; anything else ends it.
const COPY: &str = r#"echo held
n=0
while read word; do
  [ "$word" = probe ] || exit 0
  n=$((n + 1))
  if out=$(stat "$1/stall-probe-$n" 2>&1); then echo ok; else echo "$out"; fi
done"#;

/// The process holding the copied mount. Dropped, it is killed and reaped.
struct Copy {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}
impl Copy {
    fn spawn(mount: &Path) -> Self {
        let mut child = Command::new("unshare")
            .args(["-m", "--propagation", "private", "sh", "-c", COPY, "copy"])
            .arg(mount)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();
        let (send, lines) = mpsc::channel();
        // Ends when the process's output closes, whatever the test does.
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if send.send(line.unwrap_or_default()).is_err() {
                    break;
                }
            }
        });
        let mut copy = Self {
            child,
            stdin,
            lines,
        };
        assert_eq!(copy.line(), "held");
        copy
    }
    fn line(&mut self) -> String {
        self.lines
            .recv_timeout(WAIT)
            .expect("the copy answered within its wait")
    }
    fn ask(&mut self, word: &str) -> String {
        writeln!(self.stdin, "{word}").unwrap();
        self.line()
    }
    /// Asks it to leave and waits a bounded time for its own exit.
    fn leave(&mut self) -> bool {
        let _ = writeln!(self.stdin, "exit");
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_normal_unmount_stalled_at_join_ends_itself_with_one_abort_write() {
    fusectl::mount();
    let rig = Rig::new("stalled-join");
    let ready = rig.mount(1);
    let token = ready.token;
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    let mut copy = Copy::spawn(root(&ready));

    // The plain detach succeeds here; the connection outlives it, so the drain
    // stops at Join and the normal unmount ends it with one abort write.
    let done = match rig.harness.try_unmount(token) {
        Ok(done) => done,
        Err(failure) => panic!("normal unmount of a copied mount: {failure:?}"),
    };
    println!("STALLED plain unmount={:?}", done.reply);
    assert_eq!(done.reply, Reply::Unmounted(token));
    let drained = done.kernel.as_ref().expect("drain receipt");
    let forced = drained.forced.expect("normal unmount made its abort");
    assert_eq!(forced.abort, layerfs_fuse::mount::AbortWrite::Written);
    assert_eq!(
        forced.detach,
        layerfs_fuse::session::DetachAttempt::Detached
    );
    // The same abort path is attached to the successful reply (#2).
    let abort = done.abort.as_deref().expect("abort-path observation");
    assert_eq!(abort.stage, TeardownStage::Join);
    assert_eq!(abort.abort, layerfs_fuse::mount::AbortWrite::Written);
    assert!(abort.detached);
    assert!(abort.work.loops_configured > 1, "{:?}", abort.work);
    assert_eq!(abort.work.loops_exited, abort.work.loops_joined);
    assert_eq!(abort.work.loops_joined, abort.work.loops_configured);
    assert_eq!((abort.work.active, abort.work.retained), (0, 0));
    assert!(
        abort.waited <= Duration::from_secs(60),
        "{:?}",
        abort.waited
    );
    assert!(
        mount_entry(&ready.directory).is_none(),
        "the daemon's mount row remains"
    );
    assert!(
        !Path::new(&ready.directory).exists(),
        "mount directory left behind"
    );

    // The removed directory took the copy's mount with it: the connection ends
    // while the copy still runs, and the copy finds nothing there.
    until("own connection entry removed", || !connection.exists());
    let answer = copy.ask("probe");
    assert!(
        answer.contains("No such file or directory"),
        "copy's call answered {answer:?}"
    );
    assert!(copy.leave(), "the copy did not exit by itself");
    println!("COPY connection=absent_while_copy_ran probe={answer:?}");
    drop(copy);
    rig.finish();
}

fn rejected_abort(errno: Option<u32>, disposition: AbortDisposition, detail: &str) {
    fusectl::mount();
    let rig = Rig::new("stalled-abort-error");
    let ready = rig.mount(1);
    let token = ready.token;
    let descriptor = abort_write::descriptor(&ready);
    let mut copy = Copy::spawn(root(&ready));
    let service = rig.harness.service.clone();
    let (send, receive) = mpsc::channel();
    let (listen, listener) = mpsc::channel();
    let thread = thread::spawn(move || {
        match errno {
            Some(errno) => abort_write::deny_write(descriptor, errno),
            None => listen
                .send(abort_write::intercept_write(descriptor))
                .unwrap(),
        }
        let result = service.execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: true,
        }));
        send.send(result).unwrap();
    });
    if errno.is_none() {
        abort_write::acknowledge_write(listener.recv_timeout(WAIT).unwrap(), descriptor);
    }
    let result = receive.recv_timeout(Duration::from_secs(15)).unwrap();
    thread.join().unwrap();
    let custody = match result {
        Err(Failure::Retained(custody)) => *custody,
        other => panic!("rejected abort: {other:?}"),
    };
    // Capture later replies before releasing the copied mount. No operation
    // may retry the denied write or infer success from a later observation.
    let again = rig.harness.try_unmount(token);
    let forced = rig.harness.try_force(token, false);
    let evidence = rig.harness.service.retained_kernel(token).unwrap().unwrap();
    // The retained entry keeps the observation for a later reply (#2).
    let observed = rig
        .harness
        .service
        .retained_abort(token)
        .expect("retained abort observation");
    assert!(copy.leave());
    drop(copy);
    until("retained connection loops joined", || {
        let work = rig.harness.status(token).kernel.unwrap().work.unwrap();
        work.loops_configured != 0 && work.loops_joined == work.loops_configured
    });
    // Return all actual holds before asserting the expected failure receipt.
    rig.harness.stop_retained();
    drop(rig.store);
    rig.fixture.cleanup();
    let stage = if errno.is_some() {
        TeardownStage::Abort
    } else {
        TeardownStage::Join
    };
    assert_eq!(custody.stage, stage, "{custody:?}");
    let facts = custody.forced.as_ref().expect("attempted abort facts");
    assert_eq!(facts.abort, disposition);
    assert_eq!(facts.detach, DetachDisposition::Detached);
    assert_eq!(facts.commit, CommitKnowledge::Absent);
    assert_eq!(facts.fenced, 0);
    assert!(custody.detail.contains(detail), "{custody:?}");
    assert!(evidence.contains("forced: Some(Forced"), "{evidence}");
    assert_eq!(observed.stage, stage);
    assert!(observed.detached);
    assert_eq!(
        observed.abort,
        match errno {
            Some(0) => layerfs_fuse::mount::AbortWrite::Short(0),
            Some(errno) =>
                layerfs_fuse::mount::AbortWrite::Failed(nix::errno::Errno::from_raw(errno as i32)),
            None => layerfs_fuse::mount::AbortWrite::Written,
        }
    );
    for result in [again, forced] {
        let Err(Failure::Retained(later)) = result else {
            panic!("retained custody changed: {result:?}");
        };
        assert_eq!(later.stage, custody.stage);
        assert_eq!(later.detail, custody.detail);
        assert_eq!(later.forced, custody.forced);
    }
}

#[test]
fn stalled_abort_failure_preserves_errno_and_custody() {
    rejected_abort(Some(nix::libc::EIO as u32), AbortDisposition::Failed, "EIO");
}

#[test]
fn stalled_short_abort_preserves_count_and_custody() {
    rejected_abort(Some(0), AbortDisposition::Short, "0 of 1 byte");
}

fn later_failure(stage: TeardownStage) {
    fusectl::mount();
    let rig = Rig::new("stalled-later-failure");
    let ready = rig.mount(1);
    let token = ready.token;
    let mut copy = Copy::spawn(root(&ready));
    let operation = rig.harness.service.operation(token).unwrap();
    match stage {
        TeardownStage::Revoke => {
            let owner = operation.overlay().clone();
            let route = operation.workspace().route();
            // A failed owner job poisons only this Workspace's connection.
            assert!(thread::spawn(move || {
                owner.call::<(), _>(route, layerfs_daemon::ServiceClass::General, None, |_| {
                    panic!("staged owner failure before Revoke")
                })
            })
            .join()
            .unwrap()
            .is_err());
        }
        TeardownStage::Close => {
            // Stage an already-closed overlay through its public owner API.
            // Revoke accepts a closed Workspace; a second Close must refuse
            // Stale while preserving the native abort receipt.
            operation
                .overlay()
                .close(operation.workspace().route())
                .unwrap();
        }
        _ => unreachable!(),
    }
    drop(operation);
    let result = rig.harness.try_unmount(token);
    let observed = rig
        .harness
        .service
        .retained_abort(token)
        .expect("retained abort observation");
    let again = rig.harness.try_force(token, false);
    assert!(copy.leave());
    drop(copy);
    rig.harness.stop_retained();
    drop(rig.store);
    rig.fixture.cleanup();
    let Err(Failure::Retained(custody)) = result else {
        panic!("expected retained {stage:?}: {result:?}");
    };
    assert_eq!(custody.stage, stage, "{custody:?}");
    let facts = custody.forced.as_ref().expect("successful abort facts");
    assert_eq!(facts.abort, AbortDisposition::Written);
    // A stop after the abort keeps the observation and its later boundary.
    assert_eq!(observed.stage, stage);
    assert!(observed.detached);
    assert_eq!(observed.abort, layerfs_fuse::mount::AbortWrite::Written);
    assert_eq!(facts.detach, DetachDisposition::Detached);
    assert_eq!(facts.commit, CommitKnowledge::Absent);
    let Err(Failure::Retained(later)) = again else {
        panic!("expected unchanged retained custody: {again:?}");
    };
    assert_eq!(later.stage, custody.stage);
    assert_eq!(later.detail, custody.detail);
    assert_eq!(later.forced, custody.forced);
}

#[test]
fn stalled_abort_facts_survive_revoke_failure() {
    later_failure(TeardownStage::Revoke);
}

#[test]
fn stalled_abort_facts_survive_close_failure() {
    later_failure(TeardownStage::Close);
}

#[test]
fn stalled_abort_facts_survive_a_second_drain_stop() {
    rejected_abort(None, AbortDisposition::Written, "Some(Written)");
}
