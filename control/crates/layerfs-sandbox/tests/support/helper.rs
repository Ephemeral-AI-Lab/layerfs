//! A test's helper process: the test binary re-run on one helper test, which
//! pauses inside a real copy until the test releases or kills it.
//!
//! Neither side may leave the helper behind. [`Helper`] kills and reaps it
//! when the test ends, a panic included. The helper's own wait,
//! [`wait_for_release`], ends the process once its test process is gone (a
//! killed test runs no drop) and fails after a deadline, so a release that
//! never comes cannot leave it waiting forever.
use std::{
    path::Path,
    process::{Child, Command, Output},
    time::{Duration, Instant},
};

/// The environment variable that gives a helper its test process's pid.
const PARENT: &str = "LP176_HELPER_PARENT";
/// How long a paused helper waits for its release: twice the tests' longest
/// wait (30 s), well inside the 120 s test ceiling.
const RELEASE_DEADLINE: Duration = Duration::from_secs(60);

/// A running helper process, killed and reaped when dropped.
pub struct Helper(Option<Child>);

impl Helper {
    /// Starts `command`, telling the helper this process's pid.
    pub fn spawn(command: &mut Command) -> Self {
        let child = command
            .env(PARENT, std::process::id().to_string())
            .spawn()
            .expect("the helper process starts");
        Self(Some(child))
    }

    /// Kills the helper with SIGKILL, so nothing in it runs on the way out,
    /// and reaps it. A second call does nothing.
    pub fn kill_and_wait(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Waits for the helper to end by itself and returns its output.
    pub fn finish(mut self) -> Output {
        let child = self.0.take().expect("a running helper");
        child.wait_with_output().expect("the helper's output")
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.kill_and_wait();
    }
}

/// In a helper: waits until `release` exists. Within 20 ms of its test
/// process ending, the helper exits instead; after [`RELEASE_DEADLINE`] it
/// fails.
pub fn wait_for_release(release: &Path) {
    let parent: Option<u32> = std::env::var(PARENT).ok().and_then(|pid| pid.parse().ok());
    let start = Instant::now();
    while !release.exists() {
        if parent.is_some_and(|pid| std::os::unix::process::parent_id() != pid) {
            std::process::exit(0);
        }
        assert!(
            start.elapsed() < RELEASE_DEADLINE,
            "no release at {} within {RELEASE_DEADLINE:?}",
            release.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// In a helper that holds something until killed: true while its test
/// process runs and `limit` has not passed since `start`.
pub fn test_running(start: Instant, limit: Duration) -> bool {
    let parent: Option<u32> = std::env::var(PARENT).ok().and_then(|pid| pid.parse().ok());
    let alive = parent.is_none_or(|pid| std::os::unix::process::parent_id() == pid);
    alive && start.elapsed() < limit
}
