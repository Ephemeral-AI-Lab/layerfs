//! One connection never stops or holds up a worker (layerfs-project#177,
//! #179 D7): a failed accept, a connection that gets no thread, a panic in
//! an operation, a hello that trickles in, a caller that stops reading and
//! one that never closes. The failures come from the `worker-fault` cargo
//! feature (`worker_server::fault`), which also shortens the waits; this
//! file is empty without it:
//!
//! `cargo test -p layerfs-control --features worker-fault --test worker_fault`
#![cfg(feature = "worker-fault")]
use layerfs_protocol::provision::install::SdkValue;
use layerfs_api::worker::fault::{Faults, OpFault};
use layerfs_home::home::paths;
use layerfs_machine::worker::{self, Endpoint};
use layerfs_store::sqlite::transfer::step::{StepFrame, StoreStep};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

#[path = "../support/worker_store.rs"]
mod worker_store;
use worker_store::{scratch, sent, until, SECRET};

/// A worker of this build with `faults`, serving a new root.
struct Worker {
    endpoint: Endpoint,
    faults: Arc<Faults>,
    root: PathBuf,
    /// What `serve` answered, once it returns.
    stopped: mpsc::Receiver<std::io::Error>,
}

fn worker(tag: &str, faults: Faults) -> Worker {
    let root = scratch(tag).join("data");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: SECRET.to_owned(),
    };
    let faults = Arc::new(faults);
    let (stop, stopped) = mpsc::channel();
    let (serving, served) = (faults.clone(), root.clone());
    std::thread::spawn(move || {
        let ended = layerfs_api::worker::serve::serve_with(
            listener,
            SECRET.to_owned(),
            served,
            serving,
        );
        let _ = stop.send(ended.expect_err("serve returns only when its listener fails"));
    });
    Worker {
        endpoint,
        faults,
        root,
        stopped,
    }
}

impl Worker {
    fn call(&self, op: &str, args: SdkValue) -> (Result<SdkValue, (String, String)>, usize) {
        let mut events = 0;
        let outcome = worker::call(&self.endpoint, op, args, &mut |_| events += 1);
        (
            outcome.map_err(|refusal| (refusal.code, refusal.message)),
            events,
        )
    }

    fn ping(&self) -> Result<SdkValue, (String, String)> {
        worker::ping(&self.endpoint).map_err(|refusal| (refusal.code, refusal.message))
    }

    /// The logged lines that hold `part`.
    fn logged(&self, part: &str) -> Vec<String> {
        let lines = self.faults.lines();
        lines
            .into_iter()
            .filter(|line| line.contains(part))
            .collect()
    }

    fn remove(&self) {
        let _ = std::fs::remove_dir_all(self.root.parent().unwrap());
    }
}

#[test]
fn accept_failures_that_pass_are_logged_once_a_kind_and_serving_goes_on() {
    let faults = Faults::default()
        .accept_pause(Duration::from_millis(5))
        .log_every(Duration::from_millis(400));
    let passing = [
        libc::ENFILE,
        libc::ENOBUFS,
        libc::ENOMEM,
        libc::ECONNABORTED,
        libc::EINTR,
    ];
    faults.fail_accepts(&[libc::EMFILE; 5]);
    faults.fail_accepts(&passing);
    let worker = worker("accept", faults);
    // served after ten failed accepts
    worker.ping().unwrap();
    let lines = worker.logged("accept failed");
    assert_eq!(lines.len(), 6, "one line a kind: {lines:?}");
    for kind in [
        "EMFILE",
        "ENFILE",
        "ENOBUFS",
        "ENOMEM",
        "ECONNABORTED",
        "EINTR",
    ] {
        let of_kind = worker.logged(&format!("accept failed ({kind}:"));
        assert_eq!(of_kind.len(), 1, "{kind}: {lines:?}");
        assert!(of_kind[0].ends_with("still serving"), "{of_kind:?}");
    }

    // the four that were not logged are counted in the kind's next line.
    // The loop is in `accept` now, so it meets the next failure after it
    // has accepted one more connection.
    std::thread::sleep(Duration::from_millis(450));
    worker.faults.fail_accepts(&[libc::EMFILE]);
    worker.ping().unwrap();
    until(Duration::from_secs(5), "the second EMFILE line", || {
        worker.logged("accept failed (EMFILE:").len() == 2
    });
    let again = worker.logged("accept failed (EMFILE:");
    assert!(
        again[1].ends_with("still serving; 4 more since this was last logged"),
        "{again:?}"
    );
    worker.ping().unwrap();
    assert!(worker.stopped.try_recv().is_err(), "the worker stopped");
    worker.remove();
}

#[test]
fn an_accept_failure_of_the_listener_itself_ends_serve() {
    // the listed ones, and one the loop does not know: it does not guess
    for code in [libc::EBADF, libc::EINVAL, libc::ENOTSOCK, libc::EPROTO] {
        let faults = Faults::default();
        faults.fail_accepts(&[code]);
        let worker = worker("fatal", faults);
        let error = (worker.stopped)
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("serve went on after error {code}"));
        assert_eq!(error.raw_os_error(), Some(code));
        let lines = worker.logged("the worker stops");
        assert_eq!(lines.len(), 1, "{:?}", worker.faults.lines());
        worker.remove();
    }
}

#[test]
fn a_connection_that_gets_no_thread_is_closed_and_the_next_is_served() {
    let faults = Faults::default();
    faults.fail_spawns(1);
    let worker = worker("thread", faults);
    let (code, message) = worker.ping().unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("was not sent"), "{message}");
    assert_eq!(worker.ping().unwrap().get_text("kind"), Some("worker"));
    let lines = worker.logged("no thread for a connection");
    assert_eq!(lines.len(), 1, "{:?}", worker.faults.lines());
    assert!(lines[0].contains("still serving"), "{lines:?}");
    assert!(worker.stopped.try_recv().is_err(), "the worker stopped");
    worker.remove();
}

#[test]
fn a_panic_ends_its_connection_alone() {
    let worker = worker("panic", Faults::default());
    let args = || SdkValue::map([("store", SdkValue::text("ARGUMENT-NOT-LOGGED"))]);

    // a read-only operation, before any event: `Failed`
    worker.faults.on("worker.ping", OpFault::Panic);
    let (code, message) = worker.ping().unwrap_err();
    assert_eq!(code, "Failed", "{message}");
    assert!(message.contains("worker.ping panicked"), "{message}");
    assert!(message.contains("worker.log"), "{message}");
    assert_eq!(worker.logged("worker.ping panicked: ").len(), 1);

    // a read-only operation after an event: `Unknown`, as any other
    worker.faults.on("store.diagnose", OpFault::PanicAfterEvent);
    let (outcome, events) = worker.call("store.diagnose", args());
    let (code, message) = outcome.unwrap_err();
    assert_eq!((code.as_str(), events), ("Unknown", 1), "{message}");
    assert_eq!(worker.logged("store.diagnose panicked: ").len(), 1);

    // every other operation: `Unknown`, whenever it panics
    for op in [
        "store.call",
        "store.file",
        "sandbox.host",
        "project.init",
        "project.remove",
    ] {
        worker.faults.on(op, OpFault::Panic);
        let (outcome, events) = worker.call(op, args());
        let (code, message) = outcome.unwrap_err();
        assert_eq!((code.as_str(), events), ("Unknown", 0), "{op}: {message}");
        assert!(message.contains("may have run"), "{message}");
        assert_eq!(worker.logged(&format!("{op} panicked: ")).len(), 1, "{op}");
    }

    // one line a panic, and none holds what the request carried
    let lines = worker.faults.lines();
    assert_eq!(lines.len(), 7, "{lines:?}");
    assert!(
        lines.iter().all(|line| !line.contains("ARGUMENT")),
        "{lines:?}"
    );
    // the worker serves on, and no connection is left
    assert_eq!(worker.ping().unwrap().get_text("kind"), Some("worker"));
    let (refused, _) = worker.call("store.call", args());
    assert_eq!(refused.unwrap_err().0, "InvalidInput");
    until(Duration::from_secs(5), "no connection left", || {
        worker.faults.serving_now() == 0
    });
    assert!(worker.stopped.try_recv().is_err(), "the worker stopped");
    worker.remove();
}

#[test]
fn a_hung_operation_holds_up_no_other_request() {
    let worker = worker("hang", Faults::default());
    let hang = Duration::from_millis(2500);
    worker.faults.on("worker.ping", OpFault::Hang(hang));
    let started = Instant::now();
    let endpoint = worker.endpoint.clone();
    let hung = std::thread::spawn(move || worker::ping(&endpoint));
    until(Duration::from_secs(5), "the hung request", || {
        worker.faults.serving_now() == 1
    });
    // the switch is taken: this ping runs at once
    worker.ping().unwrap();
    assert!(started.elapsed() < hang, "waited for the hung request");
    assert!(!hung.is_finished());
    hung.join().unwrap().unwrap();
    assert!(started.elapsed() >= hang);
    worker.remove();
}

#[test]
fn a_trickling_hello_is_closed_at_one_deadline() {
    let deadline = Duration::from_millis(1000);
    let worker = worker("hello", Faults::default().hello_wait(deadline));
    // a hello of the right secret that takes 20 s to arrive a byte at a time
    let hello = SdkValue::map([
        ("secret", SdkValue::text(SECRET)),
        ("pad", SdkValue::text("x".repeat(200))),
    ]);
    let hello = worker::frame_bytes(&hello).unwrap();
    let mut stream = TcpStream::connect(worker.endpoint.address).unwrap();
    stream.set_nodelay(true).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let started = Instant::now();
    let mut sent = 0;
    let closed = loop {
        assert!(started.elapsed() < Duration::from_secs(10), "not closed");
        if stream.write_all(&hello[sent..sent + 1]).is_err() {
            break started.elapsed();
        }
        sent += 1;
        // each read waits 100 ms: the next byte follows it
        match stream.read(&mut [0u8; 1]) {
            Ok(0) => break started.elapsed(),
            Ok(_) => panic!("the worker answered a hello it had not read"),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break started.elapsed(),
        }
    };
    assert!(sent < hello.len(), "the whole hello was sent");
    assert!(
        closed >= deadline - Duration::from_millis(150),
        "{closed:?}"
    );
    assert!(closed < deadline * 3, "{closed:?}");
    until(Duration::from_secs(5), "no connection left", || {
        worker.faults.serving_now() == 0
    });
    // a hello inside the deadline is answered as ever
    worker.ping().unwrap();
    worker.remove();
}

/// The bytes left to read on `stream` until it ends.
fn drain(mut stream: TcpStream) -> usize {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut buffer = vec![0u8; 1 << 16];
    let mut total = 0;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return total,
            Ok(read) => total += read,
        }
    }
}

/// 64 events of 1 MiB before an operation: far more than a socket holds.
const FLOOD: OpFault = OpFault::Events {
    count: 64,
    bytes: 1 << 20,
};

#[test]
fn a_caller_that_stops_reading_is_cut_once_and_the_step_still_ends() {
    let wait = Duration::from_millis(1200);
    let worker = worker("stall", Faults::default().write_wait(wait));
    // this thread works for a home: its id goes with the Sandbox request
    layerfs_home::home::open(&worker.root.parent().unwrap().join("home")).unwrap();

    // `store.file`: the write of a piece of a Store's incoming copy
    worker.faults.on("store.file", FLOOD);
    let piece = StepFrame {
        store: "shop".to_owned(),
        store_record: None,
        step: StoreStep::Write {
            offset: 0,
            bytes: b"the piece".to_vec(),
        },
    };
    let started = Instant::now();
    let stalled = sent(&worker.endpoint, "store.file", piece.encode());
    until(Duration::from_secs(1), "the request being served", || {
        worker.faults.serving_now() == 1
    });
    let freed = until(wait * 5, "the stalled connection's thread", || {
        worker.faults.serving_now() == 0
    });
    // one wait, not one for each of the 64 events
    assert!(started.elapsed() >= wait, "{freed:?}");
    // the step ended although no event of it could be written
    let incoming = paths::store_dir(&worker.root, "shop").join("store.sqlite.incoming");
    assert_eq!(std::fs::read(&incoming).unwrap(), b"the piece");
    // the caller, reading again, finds a cut stream and no answer
    assert!(drain(stalled) < 63 << 20);

    // `sandbox.host`: the records of a Sandbox call are settled
    worker.faults.on("sandbox.host", FLOOD);
    let started = Instant::now();
    let stalled = sent(
        &worker.endpoint,
        "sandbox.host",
        worker_store::sandbox_args(),
    );
    until(Duration::from_secs(1), "the request being served", || {
        worker.faults.serving_now() == 1
    });
    until(wait * 5, "the stalled connection's thread", || {
        worker.faults.serving_now() == 0
    });
    assert!(started.elapsed() >= wait);
    assert!(paths::store_record(&worker.root, "shop").is_file());
    assert!(drain(stalled) < 63 << 20);

    // a caller that reads is never cut, however much is sent
    worker.faults.on("worker.ping", FLOOD);
    let (answer, events) = worker.call("worker.ping", SdkValue::Null);
    assert_eq!(answer.unwrap().get_text("kind"), Some("worker"));
    assert_eq!(events, 64);
    worker.remove();
}

#[test]
fn a_caller_that_never_closes_is_waited_for_once() {
    let wait = Duration::from_millis(600);
    let worker = worker("close", Faults::default().close_wait(wait));
    let started = Instant::now();
    let mut open = sent(&worker.endpoint, "worker.ping", SdkValue::Null);
    let answer = worker::read_frame(&mut open).unwrap().unwrap();
    assert!(answer.get("value").is_some(), "{answer:?}");
    assert_eq!(worker.faults.serving_now(), 1, "it waits for the close");
    until(wait * 5, "the connection's thread", || {
        worker.faults.serving_now() == 0
    });
    assert!(started.elapsed() >= wait);
    drop(open);
    worker.ping().unwrap();
    worker.remove();
}
