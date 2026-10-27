//! A normal unmount whose connection outlives its mount (issues #426, #460).
//!
//! Real kernel mount, staging, hook-free. After Attach, `unshare -m` makes a
//! second mount namespace that holds a copy of the Workspace mount, the way an
//! engine or VM agent outside a Sandbox can. The daemon's plain detach then
//! succeeds in its own namespace while the superblock lives on in the copy, so
//! the kernel never ends the connection: the receive loops stay blocked and
//! the drain stops at Join. A second normal unmount returns the same custody.
//! One forced unmount ends it with one abort write, then drains, revokes and
//! closes. Removing the mount directory then detaches the copy's mount too
//! (Linux detaches the mounts on a removed mount point in every namespace),
//! so the connection's control entry goes while the copy still runs, and the
//! copy finds no mount there.
//!
//! The safety rule of `support/fusectl.rs` binds this file: nothing here
//! writes under `/sys/fs/fuse/connections` or reads another connection.
#![cfg(target_os = "linux")]
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
    AbortDisposition, CommitKnowledge, DetachDisposition, MountPhase, Reply, TeardownStage,
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
fn a_normal_unmount_stalled_at_join_is_ended_by_one_forced_unmount() {
    fusectl::mount();
    let rig = Rig::new("stalled-join");
    let ready = rig.mount(1);
    let token = ready.token;
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    let mut copy = Copy::spawn(root(&ready));

    // The plain detach succeeds here; the connection outlives it.
    let custody = match rig.harness.try_unmount(token) {
        Err(Failure::Retained(custody)) => *custody,
        other => panic!(
            "normal unmount with a copied mount: {:?}",
            other.map(|done| done.reply)
        ),
    };
    println!("STALLED custody={custody:?}");
    assert_eq!(custody.stage, TeardownStage::Join, "{custody:?}");
    assert!(custody.detached && custody.forced.is_none(), "{custody:?}");
    let work = custody.work.expect("custody work");
    assert_eq!(
        (work.loops_exited, work.active, work.retained),
        (0, 0, 0),
        "{work:?}"
    );
    assert!(
        mount_entry(&ready.directory).is_none(),
        "the daemon's mount row remains"
    );
    assert!(connection.exists(), "the connection ended with its mount");
    assert_eq!(rig.harness.phase(token), MountPhase::Retained);

    // Plain unmount is unchanged: a second one returns the same custody.
    match rig.harness.try_unmount(token) {
        Err(Failure::Retained(again)) => assert_eq!(
            (again.stage, again.detail.as_str()),
            (custody.stage, custody.detail.as_str())
        ),
        other => panic!("second normal unmount: {:?}", other.map(|done| done.reply)),
    }

    // One forced unmount ends the stalled custody.
    let outcome = match rig.harness.try_force(token, false) {
        Ok(done) => match done.reply {
            Reply::ForceUnmounted(closed) => {
                assert_eq!(closed.token, token);
                closed.outcome
            }
            other => panic!("forced unmount replied {other:?}"),
        },
        Err(failure) => panic!("forced unmount of a stalled Join: {failure:?}"),
    };
    println!("FORCED outcome={outcome:?}");
    assert_eq!(outcome.facts.abort, AbortDisposition::Written);
    assert_eq!(outcome.facts.detach, DetachDisposition::Detached);
    assert_eq!(outcome.facts.commit, CommitKnowledge::Absent);
    assert!(
        outcome.work.loops_configured != 0
            && outcome.work.loops_joined == outcome.work.loops_configured,
        "{:?}",
        outcome.work
    );
    assert!(
        !Path::new(&ready.directory).exists(),
        "mount directory left behind"
    );

    // The removed directory took the copy's mount with it: the connection
    // ends while the copy still runs, and the copy finds nothing there.
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
