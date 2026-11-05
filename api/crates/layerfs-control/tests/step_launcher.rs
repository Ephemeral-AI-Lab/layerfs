//! Contract tests for the real launcher adapter (#555): one command is a
//! real detached process, its pid, output and exit receipt live under
//! `step/launch/`, and a launcher instance that did not start it observes the
//! same receipt. `step_runtime.rs` keeps the clock and launcher doubles;
//! these tests run real commands.
use layerfs_control::step::{
    CancelOutcome, ExecId, ExecRequest, ExecStatus, Launcher, Observation, ProcessLauncher,
    StepRuntime, SystemClock, WaitStore,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A temporary home, removed at the end of the test.
struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("layerfs-step-process-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn store(&self) -> WaitStore {
        WaitStore::open(self.0.join("step"))
    }

    fn receipt(&self, id: &ExecId) -> PathBuf {
        self.0.join("step").join("launch").join(id.as_str())
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn runtime(temp: &Temp) -> StepRuntime<ProcessLauncher, SystemClock> {
    StepRuntime::new(temp.store(), ProcessLauncher::in_home(&temp.0), SystemClock)
}

/// A path as one shell word.
fn quoted(path: &Path) -> String {
    format!("'{}'", path.display())
}

/// Waits until `done` answers true, for at most `limit`.
fn until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

/// The exit code the wrapper wrote beside `id`, once it has.
fn exit_of(temp: &Temp, id: &ExecId) -> Option<i32> {
    fs::read_to_string(temp.receipt(id).join("exit"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn start(
    runtime: &StepRuntime<ProcessLauncher, SystemClock>,
    key: &str,
    command: &str,
    timeout_ms: Option<u64>,
) -> ExecId {
    runtime
        .exec_async(ExecRequest::new(key, command, timeout_ms).unwrap())
        .unwrap()
        .exec_id
}

#[test]
fn the_adapter_observes_through_the_receipt_files() {
    let temp = Temp::new("observe");
    let launcher = ProcessLauncher::in_home(&temp.0);
    let id = ExecId::parse("exec-0000000000000001").unwrap();
    let handle = launcher
        .start(&id, &ExecRequest::new("k", "echo done", None).unwrap())
        .unwrap();
    assert!(until(Duration::from_secs(10), || launcher.observe(&handle)
        == Observation::Ended(0)));
    let out = fs::read_to_string(temp.receipt(&id).join("stdout")).unwrap();
    assert_eq!(out.trim(), "done");
    // a launcher that did not start the command reads the same receipt
    assert_eq!(
        ProcessLauncher::in_home(&temp.0).observe(&handle),
        Observation::Ended(0)
    );
}

#[test]
fn async_wait_and_status_run_a_real_command() {
    let temp = Temp::new("run");
    let runtime = runtime(&temp);
    let started = runtime
        .exec_async(ExecRequest::new("k", "echo hello; exit 3", Some(60_000)).unwrap())
        .unwrap();
    assert!(!started.reused);
    assert_eq!(started.status, ExecStatus::Running);
    assert_eq!(
        runtime
            .exec_wait(&started.exec_id, Duration::from_secs(10))
            .unwrap(),
        ExecStatus::Failed { exit_code: 3 }
    );
    assert_eq!(
        runtime.exec_status(&started.exec_id).unwrap(),
        ExecStatus::Failed { exit_code: 3 }
    );
    assert_eq!(
        runtime.receipt(&started.exec_id).unwrap().unwrap().status,
        ExecStatus::Failed { exit_code: 3 }
    );
    let out = fs::read_to_string(temp.receipt(&started.exec_id).join("stdout")).unwrap();
    assert_eq!(out.trim(), "hello");
}

#[test]
fn a_new_launcher_resumes_a_live_command() {
    let temp = Temp::new("restart");
    let gate = temp.0.join("gate");
    let marker = temp.0.join("marker");
    let command = format!(
        "while [ ! -f {} ]; do sleep 0.02; done; echo done > {}",
        quoted(&gate),
        quoted(&marker)
    );
    let id = {
        let first = runtime(&temp);
        start(&first, "k", &command, None)
    };
    // a second launcher holds no in-memory wait: the durable entry and the
    // receipt files are the wait
    let second = runtime(&temp);
    assert_eq!(second.exec_status(&id).unwrap(), ExecStatus::Running);
    fs::write(&gate, b"go").unwrap();
    assert_eq!(
        second.exec_wait(&id, Duration::from_secs(10)).unwrap(),
        ExecStatus::Succeeded { exit_code: 0 }
    );
    assert!(marker.is_file());
}

#[test]
fn cancel_ends_a_real_command_and_keeps_its_outcome() {
    let temp = Temp::new("cancel");
    let marker = temp.0.join("after");
    let command = format!("sleep 30; echo after > {}", quoted(&marker));
    let runtime = runtime(&temp);
    let id = start(&runtime, "k", &command, Some(60_000));
    assert_eq!(runtime.cancel(&id).unwrap(), CancelOutcome::Cancelled);
    assert_eq!(runtime.exec_status(&id).unwrap(), ExecStatus::Cancelled);
    assert!(until(Duration::from_secs(10), || exit_of(&temp, &id) == Some(143)));
    assert!(!marker.is_file());
}

#[test]
fn a_passed_deadline_is_timed_out_not_cancelled() {
    let temp = Temp::new("timeout");
    let runtime = runtime(&temp);
    let id = start(&runtime, "k", "sleep 30", Some(50));
    assert_eq!(
        runtime.exec_wait(&id, Duration::from_secs(10)).unwrap(),
        ExecStatus::TimedOut
    );
    assert_eq!(runtime.cancel(&id).unwrap(), CancelOutcome::TimedOut);
    assert_ne!(CancelOutcome::TimedOut, CancelOutcome::Cancelled);
    assert!(until(Duration::from_secs(10), || exit_of(&temp, &id) == Some(143)));
}

#[test]
fn a_retry_starts_no_second_command() {
    let temp = Temp::new("retry");
    let counter = temp.0.join("count");
    let command = format!("echo x >> {}", quoted(&counter));
    let runtime = runtime(&temp);
    let first = runtime
        .exec_async(ExecRequest::new("k", &command, None).unwrap())
        .unwrap();
    let retry = runtime
        .exec_async(ExecRequest::new("k", &command, None).unwrap())
        .unwrap();
    assert_eq!(first.exec_id, retry.exec_id);
    assert!(retry.reused);
    assert_ne!(retry.status, ExecStatus::Unknown);
    assert_eq!(
        runtime
            .exec_wait(&first.exec_id, Duration::from_secs(10))
            .unwrap(),
        ExecStatus::Succeeded { exit_code: 0 }
    );
    assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 1);
}
