//! Contract tests for the step runtime (#555): async start, wait, status,
//! cancel, restart resume, cancel versus timeout, and a retry that never
//! runs a second copy. The launcher is a local double and the clock is
//! manual, so every case is deterministic and no command is run.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_control::step::{
    CancelOutcome, Clock, ExecId, ExecRequest, ExecStatus, Launcher, Observation, StepRuntime,
    WaitStore, MAXIMUM_KEY_BYTES,
};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// A temporary store root, removed at the end of the test.
struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("layerfs-step-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn store(&self) -> WaitStore {
        WaitStore::open(self.0.join("step"))
    }

    fn entry(&self, id: &ExecId) -> PathBuf {
        self.0.join("step").join(format!("{id}.json"))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Job {
    Running,
    Ended(i32),
    Gone,
}

#[derive(Default)]
struct Inner {
    starts: usize,
    cancels: usize,
    next: u64,
    jobs: BTreeMap<u64, Job>,
    fail_start: bool,
}

/// The command world a launcher double observes: it survives a runtime
/// restart, like the process it stands for.
#[derive(Clone, Default)]
struct World(Rc<RefCell<Inner>>);

impl World {
    fn starts(&self) -> usize {
        self.0.borrow().starts
    }

    fn cancels(&self) -> usize {
        self.0.borrow().cancels
    }

    fn fail_start(&self, fail: bool) {
        self.0.borrow_mut().fail_start = fail;
    }

    fn finish_running(&self, code: i32) {
        for job in self.0.borrow_mut().jobs.values_mut() {
            if *job == Job::Running {
                *job = Job::Ended(code);
            }
        }
    }

    fn end_all(&self, code: i32) {
        for job in self.0.borrow_mut().jobs.values_mut() {
            *job = Job::Ended(code);
        }
    }

    fn job_of(&self, handle: &SdkValue) -> u64 {
        u64::try_from(handle.get_int("job").unwrap()).unwrap()
    }
}

#[derive(Clone)]
struct Fake {
    world: World,
}

impl Launcher for Fake {
    fn start(&self, _id: &ExecId, request: &ExecRequest) -> Result<SdkValue, SdkRefusal> {
        let mut inner = self.world.0.borrow_mut();
        inner.starts += 1;
        if inner.fail_start {
            return Err(SdkRefusal {
                code: "Failed".to_owned(),
                message: "the local launcher refused the start".to_owned(),
                detail: SdkValue::Null,
            });
        }
        let job = match request.command.as_str() {
            "end:0" => Job::Ended(0),
            "end:7" => Job::Ended(7),
            "gone" => Job::Gone,
            _ => Job::Running,
        };
        let number = inner.next;
        inner.next += 1;
        inner.jobs.insert(number, job);
        Ok(SdkValue::map([("job", SdkValue::Int(number as i64))]))
    }

    fn observe(&self, handle: &SdkValue) -> Observation {
        let number = self.world.job_of(handle);
        match self.world.0.borrow().jobs.get(&number).copied() {
            Some(Job::Running) => Observation::Running,
            Some(Job::Ended(code)) => Observation::Ended(code),
            Some(Job::Gone) | None => Observation::Gone,
        }
    }

    fn cancel(&self, handle: &SdkValue) -> Result<(), SdkRefusal> {
        let number = self.world.job_of(handle);
        let mut inner = self.world.0.borrow_mut();
        inner.cancels += 1;
        inner.jobs.insert(number, Job::Gone);
        Ok(())
    }
}

/// A clock the test moves by hand, so no deadline needs a real sleep.
#[derive(Clone)]
struct ManualClock(Rc<Cell<i64>>);

impl ManualClock {
    fn at(now: i64) -> Self {
        Self(Rc::new(Cell::new(now)))
    }

    fn set(&self, now: i64) {
        self.0.set(now);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.get()
    }
}

type Runtime = StepRuntime<Fake, ManualClock>;

fn runtime(temp: &Temp, world: &World, clock: &ManualClock) -> Runtime {
    StepRuntime::new(
        temp.store(),
        Fake {
            world: world.clone(),
        },
        clock.clone(),
    )
}

fn start(rt: &Runtime, key: &str, command: &str, timeout_ms: Option<u64>) -> ExecId {
    rt.exec_async(ExecRequest::new(key, command, timeout_ms).unwrap())
        .unwrap()
        .exec_id
}

#[test]
fn the_contract_checks_its_inputs() {
    assert!(ExecRequest::new("key", "true", None).is_ok());
    assert_eq!(
        ExecRequest::new("", "true", None).unwrap_err().code,
        "InvalidInput"
    );
    assert_eq!(
        ExecRequest::new("key", "", None).unwrap_err().code,
        "InvalidInput"
    );
    assert_eq!(
        ExecRequest::new("k".repeat(MAXIMUM_KEY_BYTES + 1), "true", None)
            .unwrap_err()
            .code,
        "InvalidInput"
    );
    assert_eq!(
        ExecRequest::new("key", "true", Some(u64::MAX))
            .unwrap_err()
            .code,
        "InvalidInput"
    );
    let id = ExecId::parse("exec-0123456789abcdef").unwrap();
    assert_eq!(id.as_str(), "exec-0123456789abcdef");
    for bad in [
        "0123456789abcdef",
        "exec-0123456789ABCDEF",
        "exec-0123456789abcde",
        "exec-0123456789abcdef0",
        "../exec-0123456789abcdef",
    ] {
        assert_eq!(ExecId::parse(bad).unwrap_err().code, "InvalidInput");
    }
}

#[test]
fn async_start_returns_at_once_and_status_resolves_the_outcome() {
    let temp = Temp::new("async");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);

    let started = rt
        .exec_async(ExecRequest::new("k1", "end:0", None).unwrap())
        .unwrap();
    assert!(!started.reused);
    assert_eq!(started.status, ExecStatus::Running);
    assert_eq!(world.starts(), 1);
    // the durable entry exists before the outcome does
    assert!(temp.entry(&started.exec_id).is_file());

    assert_eq!(
        rt.exec_status(&started.exec_id).unwrap(),
        ExecStatus::Succeeded { exit_code: 0 }
    );
    let receipt = rt.receipt(&started.exec_id).unwrap().unwrap();
    assert_eq!(receipt.key, "k1");
    assert_eq!(receipt.command, "end:0");
    assert_eq!(receipt.status, ExecStatus::Succeeded { exit_code: 0 });
    assert_eq!(receipt.started, 1_000);
}

#[test]
fn wait_observes_until_the_command_ends() {
    let temp = Temp::new("wait");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);
    let id = start(&rt, "k", "run", None);

    // a zero wait observes once and returns, leaving the entry running
    assert_eq!(
        rt.exec_wait(&id, Duration::ZERO).unwrap(),
        ExecStatus::Running
    );
    world.finish_running(0);
    assert_eq!(
        rt.exec_wait(&id, Duration::from_secs(1)).unwrap(),
        ExecStatus::Succeeded { exit_code: 0 }
    );
}

#[test]
fn a_nonzero_exit_is_failed_with_its_code() {
    let temp = Temp::new("failed");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);
    let id = start(&rt, "k", "end:7", None);
    assert_eq!(
        rt.exec_wait(&id, Duration::ZERO).unwrap(),
        ExecStatus::Failed { exit_code: 7 }
    );
    assert_eq!(
        rt.receipt(&id).unwrap().unwrap().status,
        ExecStatus::Failed { exit_code: 7 }
    );
}

#[test]
fn cancel_and_timeout_are_distinct_outcomes() {
    let temp = Temp::new("cancel-timeout");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);

    // a cancellation recorded before the deadline is Cancelled
    let cancelled = start(&rt, "k-cancel", "run", Some(60_000));
    assert_eq!(rt.cancel(&cancelled).unwrap(), CancelOutcome::Cancelled);
    assert_eq!(
        rt.exec_wait(&cancelled, Duration::ZERO).unwrap(),
        ExecStatus::Cancelled
    );
    assert_eq!(rt.cancel(&cancelled).unwrap(), CancelOutcome::Cancelled);
    assert_eq!(world.cancels(), 1);

    // a deadline that passed first is TimedOut, and a later cancel keeps it
    let timed_out = start(&rt, "k-timeout", "run", Some(100));
    clock.set(1_100);
    assert_eq!(
        rt.exec_wait(&timed_out, Duration::ZERO).unwrap(),
        ExecStatus::TimedOut
    );
    assert_eq!(rt.cancel(&timed_out).unwrap(), CancelOutcome::TimedOut);
    assert_eq!(
        rt.exec_wait(&timed_out, Duration::ZERO).unwrap(),
        ExecStatus::TimedOut
    );
    assert_ne!(CancelOutcome::Cancelled, CancelOutcome::TimedOut);
    // the timeout signalled the command once; the cancel did not do it again
    assert_eq!(world.cancels(), 2);
}

#[test]
fn a_retry_never_starts_a_second_copy() {
    let temp = Temp::new("retry");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);

    let first = rt
        .exec_async(ExecRequest::new("k", "run", None).unwrap())
        .unwrap();
    let retry = rt
        .exec_async(ExecRequest::new("k", "run", None).unwrap())
        .unwrap();
    assert_eq!(first.exec_id, retry.exec_id);
    assert!(retry.reused);
    assert_eq!(retry.status, ExecStatus::Running);
    assert_eq!(world.starts(), 1);

    world.finish_running(0);
    assert_eq!(
        rt.exec_wait(&first.exec_id, Duration::ZERO).unwrap(),
        ExecStatus::Succeeded { exit_code: 0 }
    );
    // after it ended, a retry answers the same receipt and starts nothing
    let after = rt
        .exec_async(ExecRequest::new("k", "run", None).unwrap())
        .unwrap();
    assert_eq!(after.exec_id, first.exec_id);
    assert!(after.reused);
    assert_eq!(after.status, ExecStatus::Succeeded { exit_code: 0 });
    assert_eq!(world.starts(), 1);

    // the same key with another command is refused, not started
    let conflict = rt
        .exec_async(ExecRequest::new("k", "other", None).unwrap())
        .unwrap_err();
    assert_eq!(conflict.code, "Conflict");
    assert_eq!(world.starts(), 1);
}

#[test]
fn a_new_launcher_resumes_the_wait_from_the_durable_entry() {
    let temp = Temp::new("restart");
    let world = World::default();
    let clock = ManualClock::at(1_000);

    let id = {
        let first = runtime(&temp, &world, &clock);
        start(&first, "k", "run", None)
    };

    // a later launcher holds no in-memory wait: it reads the entry
    let second = runtime(&temp, &world, &clock);
    assert_eq!(world.starts(), 1);
    assert_eq!(
        second.exec_wait(&id, Duration::ZERO).unwrap(),
        ExecStatus::Running
    );
    world.finish_running(0);
    assert_eq!(
        second.exec_wait(&id, Duration::from_secs(1)).unwrap(),
        ExecStatus::Succeeded { exit_code: 0 }
    );
    let receipt = second.receipt(&id).unwrap().unwrap();
    assert_eq!(receipt.key, "k");
    assert_eq!(receipt.command, "run");
    assert_eq!(world.starts(), 1);
}

#[test]
fn a_gone_process_is_unresolved_and_never_replayed() {
    let temp = Temp::new("unresolved");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);
    let id = start(&rt, "k", "gone", None);

    assert_eq!(
        rt.exec_wait(&id, Duration::ZERO).unwrap(),
        ExecStatus::Unresolved
    );
    // the command later appears to have ended, but the recorded outcome
    // stays unresolved: an unknown outcome is never replayed
    world.end_all(0);
    assert_eq!(rt.exec_status(&id).unwrap(), ExecStatus::Unresolved);
    let retry = rt
        .exec_async(ExecRequest::new("k", "gone", None).unwrap())
        .unwrap();
    assert!(retry.reused);
    assert_eq!(retry.status, ExecStatus::Unresolved);
    assert_eq!(world.starts(), 1);
}

#[test]
fn a_cancel_that_finds_the_command_finished_changes_nothing() {
    let temp = Temp::new("finished");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);
    let id = start(&rt, "k", "end:0", None);

    assert_eq!(rt.cancel(&id).unwrap(), CancelOutcome::Unknown);
    assert_eq!(
        rt.receipt(&id).unwrap().unwrap().status,
        ExecStatus::Succeeded { exit_code: 0 }
    );
    assert_eq!(world.cancels(), 0);
}

#[test]
fn a_failed_start_is_recorded_and_not_replayed() {
    let temp = Temp::new("failed-start");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);

    world.fail_start(true);
    let error = rt
        .exec_async(ExecRequest::new("k", "run", None).unwrap())
        .unwrap_err();
    assert_eq!(error.code, "Failed");
    assert_eq!(world.starts(), 1);

    world.fail_start(false);
    let retry = rt
        .exec_async(ExecRequest::new("k", "run", None).unwrap())
        .unwrap();
    assert!(retry.reused);
    assert_eq!(retry.status, ExecStatus::Unresolved);
    assert_eq!(world.starts(), 1);
}

#[test]
fn an_unknown_id_has_no_status_and_no_cancellation() {
    let temp = Temp::new("unknown");
    let world = World::default();
    let clock = ManualClock::at(1_000);
    let rt = runtime(&temp, &world, &clock);
    let id = ExecId::parse("exec-0000000000000000").unwrap();

    assert_eq!(rt.exec_status(&id).unwrap(), ExecStatus::Unknown);
    assert_eq!(
        rt.exec_wait(&id, Duration::ZERO).unwrap(),
        ExecStatus::Unknown
    );
    assert_eq!(rt.cancel(&id).unwrap(), CancelOutcome::Unknown);
    assert!(rt.receipt(&id).unwrap().is_none());
    assert_eq!(world.starts(), 0);
}
