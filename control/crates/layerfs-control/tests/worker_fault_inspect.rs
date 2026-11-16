//! A stuck, panicking or silent `inspect` on a worker that holds a real
//! Store (layerfs-project#178 isolation rules 3 and 4, #179 E): two stuck
//! reads hold two helper threads and nothing else, the third is `Busy`,
//! Store and Sandbox operations go on, descriptors come back; a panic is
//! `Failed`; and a worker that never answers is `Unavailable` at the
//! caller's budget. The faults come from the `worker-fault` cargo feature;
//! this file is empty without it:
//!
//! `cargo test -p layerfs-control --features worker-fault --test worker_fault_inspect`
//!
//! The worker is in this process, whose inspect permits it shares with
//! every test here, so each takes one lock.
#![cfg(feature = "worker-fault")]
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_control::worker_server::fault::{Faults, OpFault, INSPECT_HELPER};
use layerfs_home::names::MachineName;
use layerfs_machine::worker;
use layerfs_sandbox::inspect::{self, Window, HARD};
use layerfs_store::place;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[path = "support/worker_store.rs"]
mod worker_store;
use worker_store::{placed, until, Placed, SECRET};

static PROCESS: Mutex<()> = Mutex::new(());

/// The test's turn, once no earlier test's helper is still running.
fn serial() -> MutexGuard<'static, ()> {
    let guard = PROCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    until(Duration::from_secs(15), "earlier helpers", || {
        inspect::running() == 0
    });
    guard
}

fn with(tag: &str) -> (Placed, Arc<Faults>) {
    let faults = Arc::new(Faults::default());
    let serving = faults.clone();
    let placed = placed(tag, |listener, root| {
        std::thread::spawn(move || {
            layerfs_control::worker_server::serve_with(listener, SECRET.to_owned(), root, serving)
        });
    });
    (placed, faults)
}

/// `logs.machine` of machine `box`, as the call runs it: through the
/// inspect route, waiting at most the caller's budget.
fn machine_log(home: &Path) -> Result<SdkValue, SdkRefusal> {
    let machine = MachineName::parse("box").unwrap();
    inspect::machine_log(home, &machine, &Window::default())
}

/// How many descriptors this process holds open.
fn descriptors() -> usize {
    std::fs::read_dir("/dev/fd").unwrap().count()
}

/// How many threads this process has, where the kernel lists them.
fn threads() -> Option<usize> {
    Some(std::fs::read_dir("/proc/self/task").ok()?.count())
}

#[test]
fn two_stuck_reads_hold_two_helpers_and_nothing_else() {
    let _turn = serial();
    let (placed, faults) = with("inspect-stuck");
    let home = placed.home.clone();
    // as it answers with nothing wrong: an empty log, a Store call
    assert!(machine_log(&home).unwrap().get("lines").is_some());
    let before = worker_store::stats(&home).unwrap();
    let _ = worker_store::sandbox_call(&placed.endpoint);
    until(Duration::from_secs(2), "the first connections' end", || {
        faults.serving_now() == 0
    });
    let (open, running) = (descriptors(), threads());

    // two reads stuck in the worker for 8 s, each on its helper thread
    let stuck = Duration::from_secs(8);
    assert!(stuck > HARD && place::READ.answer > HARD);
    faults.on(INSPECT_HELPER, OpFault::Hang(stuck));
    faults.on(INSPECT_HELPER, OpFault::Hang(stuck));
    let (done, answers) = mpsc::channel();
    let started = Instant::now();
    for _ in 0..2 {
        let (done, home) = (done.clone(), home.clone());
        std::thread::spawn(move || {
            let _ = done.send((machine_log(&home), started.elapsed()));
        });
    }
    until(Duration::from_secs(2), "both admitted", || {
        inspect::running() == 2
    });

    // the third is refused at once
    let asked = Instant::now();
    let third = machine_log(&home).unwrap_err();
    assert_eq!(third.code, "Busy", "{third:?}");
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "{:?}",
        asked.elapsed()
    );
    // every other operation goes on: a Store call, a Sandbox operation, a ping
    let asked = Instant::now();
    let during = worker_store::stats(&home).unwrap();
    assert_eq!(during.get_text("name"), before.get_text("name"));
    let sandbox = worker_store::sandbox_call(&placed.endpoint).unwrap_err();
    assert_eq!(sandbox.code, "NoSuchSandbox", "{sandbox:?}");
    worker::ping(&placed.endpoint).unwrap();
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "{:?}",
        asked.elapsed()
    );

    // each stuck read's caller is answered at the worker's hard deadline
    for _ in 0..2 {
        let (outcome, after) = answers.recv_timeout(Duration::from_secs(9)).unwrap();
        let refusal = outcome.unwrap_err();
        assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
        assert!(refusal.message.contains("abandoned"), "{refusal:?}");
        assert!(after >= HARD && after < stuck, "{after:?}");
    }
    // what is left of them is the two helpers: no connection is being
    // served, and the process has at most two threads more than before
    until(Duration::from_secs(1), "the connections' end", || {
        faults.serving_now() == 0
    });
    assert_eq!(inspect::running(), 2);
    // (a thread that has answered is listed until it has ended: the two
    // callers above and the connections' own threads get a second for that)
    if let Some(then) = running {
        until(Duration::from_secs(1), "the other threads' end", || {
            threads().is_none_or(|now| now <= then + 2)
        });
    }
    // which still hold both permits, while Store calls still answer
    assert_eq!(machine_log(&home).unwrap_err().code, "Busy");
    worker_store::stats(&home).unwrap();

    // once the stuck reads end, the next is admitted and nothing is left open
    until(Duration::from_secs(4), "the helpers' end", || {
        inspect::running() == 0
    });
    assert!(started.elapsed() >= stuck);
    assert!(machine_log(&home).unwrap().get("lines").is_some());
    until(Duration::from_secs(2), "the descriptors' close", || {
        faults.serving_now() == 0 && descriptors() == open
    });
    if let (Some(now), Some(then)) = (threads(), running) {
        assert!(now <= then, "{then} threads before, {now} now");
    }
    placed.remove();
}

#[test]
fn a_panic_in_an_inspect_request_is_failed_and_the_next_is_served() {
    let _turn = serial();
    let (placed, faults) = with("inspect-panic");
    let home = placed.home.clone();
    // on the connection's thread, before any byte: read-only, so `Failed`
    faults.on(inspect::OP, OpFault::Panic);
    let refusal = machine_log(&home).unwrap_err();
    assert_eq!(refusal.code, "Failed", "{refusal:?}");
    assert!(
        refusal
            .message
            .contains("inspect panicked in the worker and changed nothing"),
        "{refusal:?}"
    );
    let lines = faults.lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("inspect panicked: "), "{lines:?}");
    // on its helper thread, under its permit: `Failed`, and the permit is free
    faults.on(INSPECT_HELPER, OpFault::Panic);
    let refusal = machine_log(&home).unwrap_err();
    assert_eq!(refusal.code, "Failed", "{refusal:?}");
    assert!(
        refusal
            .message
            .contains("an inspect request panicked and changed nothing"),
        "{refusal:?}"
    );
    assert_eq!(inspect::running(), 0);
    // after an event the connection closes instead, and a read reports that
    // as `Unavailable`: it changed nothing, so it is never `Unknown`
    faults.on(inspect::OP, OpFault::PanicAfterEvent);
    let refusal = machine_log(&home).unwrap_err();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    // the next request, and a Store call, answer
    assert!(machine_log(&home).unwrap().get("lines").is_some());
    worker_store::stats(&home).unwrap();
    placed.remove();
}

#[test]
fn a_worker_that_never_answers_is_unavailable_at_the_caller_s_budget() {
    let _turn = serial();
    let (placed, faults) = with("inspect-silent");
    let home = placed.home.clone();
    let budget = place::READ.answer;
    assert_eq!(budget, Duration::from_secs(8));
    // held on the connection's thread, before admission: no answer comes
    faults.on(inspect::OP, OpFault::Hang(budget + Duration::from_secs(3)));
    let started = Instant::now();
    let refusal = machine_log(&home).unwrap_err();
    let after = started.elapsed();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert!(refusal.message.contains("did not answer"), "{refusal:?}");
    assert!(
        after >= budget && after < budget + Duration::from_secs(2),
        "{after:?}"
    );
    // the worker itself is not held up
    assert!(machine_log(&home).unwrap().get("lines").is_some());
    worker_store::stats(&home).unwrap();
    // the held connection ends by itself, its caller long gone; waited for
    // here so the next test counts no descriptor of this one
    until(Duration::from_secs(5), "the held connection's end", || {
        faults.serving_now() == 0
    });
    placed.remove();
}
