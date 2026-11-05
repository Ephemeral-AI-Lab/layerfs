//! The worker's side: `layerfs worker` serves one request per connection.
//!
//! A connection starts with a small frame `{ secret }`: the worker reads at
//! most [`MAX_HELLO`] bytes, the whole frame within [`HELLO_WAIT`], and
//! answers `{ ready: true, protocol, bundle }` ([`Hello`]), or refuses a
//! wrong secret and closes, before it reads anything larger. A caller of
//! another build sends nothing more and closes. Then a request `{ op, args }`,
//! read with at most [`REQUEST_WAIT`] between bytes. Once it is read, the
//! worker claims it (`{ claimed: true, op }`) and then reports operation
//! progress (`{ progress: { seq, op, phase, ... } }`) before and after the
//! operation, and from its own path while it runs (a phase, a chunk of a
//! command's output). It also beats (`{ beat: true }`) every [`HEARTBEAT`]
//! while the operation runs: the beat says the process is alive, progress
//! says the operation is advancing, and the caller times the operation on
//! the latter only. `worker.ping` answers who it is;
//! `store.call`, `store.file`, `sandbox.host`, `project.stage`,
//! `project.init` and `project.remove` run one operation on this machine,
//! with `home` set to the worker's own root (the machine's data disk),
//! never a path the caller sends. Answers are
//! `{ value }` or `{ refused }`, after any `{ progress }` or `{ event }`
//! frames.
//!
//! A request may carry `home_id`, the caller's home id: the root is not a
//! home and has no id of its own (O33), so host code that asks for one
//! gets the caller's, and the root is never converted or written as a home.
use super::client::{HEARTBEAT, HELLO_WAIT, SEND_WAIT};
use super::frame::{read_frame, read_frame_within_wait, write_frame, MAX_HELLO};
use super::hello::Hello;
use crate::common::outcome::{refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// The longest the worker waits for the next byte of a request.
pub const REQUEST_WAIT: Duration = Duration::from_secs(30);

/// What a worker served before it stopped.
#[derive(Debug, Default)]
pub struct Served {
    /// Requests answered.
    pub requests: u64,
}

/// Serves `listener` until it fails or the process is ended (`machine.stop`;
/// there is no idle stop, #535 D11). `root` is the worker's own state root.
pub fn serve(listener: TcpListener, secret: String, root: PathBuf) -> std::io::Result<Served> {
    // the root holds Stores and records: private, as a home is (a machine's
    // data disk mounts 0755), but passable by others (0701), since a
    // namespaces Sandbox's commands run as another identity below it
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o701)
            .create(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o701))?;
    }
    crate::common::home::guest::serve_root(&root);
    crate::common::home::guest::retire_layout_2(&root);
    let secret = Arc::new(secret);
    let root = Arc::new(root);
    // what it answers every secret with, read once
    let ready = Arc::new(Hello::own().to_frame());
    let served = Arc::new(AtomicU64::new(0));
    for stream in listener.incoming() {
        let stream = stream?;
        let (secret, root, served) = (secret.clone(), root.clone(), served.clone());
        let ready = ready.clone();
        std::thread::spawn(move || {
            if answer(stream, &secret, &ready, &root) {
                served.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
    Ok(Served {
        requests: served.load(Ordering::Relaxed),
    })
}

/// Equal secrets, compared in time independent of where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn answer(mut stream: TcpStream, secret: &str, ready: &SdkValue, root: &Path) -> bool {
    let _ = stream.set_nodelay(true);
    // the secret first, from a small frame: nothing larger is read, and the
    // whole frame, not each byte, is waited for at most HELLO_WAIT
    let hello = match read_frame_within_wait(&mut stream, MAX_HELLO, HELLO_WAIT) {
        Ok(Some(hello)) => hello,
        _ => return false,
    };
    if !same(hello.get_text("secret").unwrap_or(""), secret) {
        let denied = refused("Denied", "the worker's secret does not match");
        let _ = write_frame(&mut stream, &refusal_frame(denied));
        return false;
    }
    if write_frame(&mut stream, ready).is_err()
        || stream.set_read_timeout(Some(REQUEST_WAIT)).is_err()
    {
        return false;
    }
    let request = match read_frame(&mut stream) {
        Ok(Some(request)) => request,
        _ => return false,
    };
    let _ = stream.set_read_timeout(None);
    if stream.set_write_timeout(Some(SEND_WAIT)).is_err() {
        return false;
    }
    // writes from the beats and the answer share one stream: a frame is
    // never interleaved with another
    let stream = Arc::new(Mutex::new(stream));
    // the request is read: claim it once, with its operation, so its caller
    // stops waiting to be heard and can tell a worker that never got going
    // from one that is running. Beats then carry its liveness until the
    // answer is ready
    let op = request.get_text("op").unwrap_or("").to_owned();
    let args = request.get("args").cloned().unwrap_or_default();
    let _home = crate::common::home::guest::sent(request.get_text("home_id"));
    if write_locked(&stream, &claimed_frame(&op)).is_err() {
        return false;
    }
    let beats = Beats::start(stream.clone());
    let mut seq = 0u64;
    let _ = write_locked(&stream, &progress_frame(&mut seq, &op, "start", Vec::new()));
    let started = Instant::now();
    let outcome = {
        // the operation's own path reports progress by sending an event
        // `{ progress: ... }`; it becomes a top-level progress frame. Any
        // other event (a command's output) goes as it came, and also shows
        // the operation is progressing.
        let mut events = |event: SdkValue| {
            let frame = match event.get("progress") {
                Some(value) => progress_event(&mut seq, &op, value.clone()),
                None => SdkValue::map([("event", event)]),
            };
            let _ = write_locked(&stream, &frame);
        };
        run(&op, args, root, &mut events)
    };
    // stop beating before the progress-done and the one answer: a caller
    // never reads a beat after them
    drop(beats);
    let _ = write_locked(
        &stream,
        &progress_frame(
            &mut seq,
            &op,
            "done",
            vec![("elapsed_ms", SdkValue::Int(ms(started.elapsed())))],
        ),
    );
    let frame = match outcome {
        Ok(value) => SdkValue::map([("value", value)]),
        Err(refusal) => refusal_frame(refusal),
    };
    write_locked(&stream, &frame).is_ok()
}

/// Writes one frame under the stream's lock.
fn write_locked(stream: &Arc<Mutex<TcpStream>>, value: &SdkValue) -> std::io::Result<()> {
    let mut stream = stream.lock().unwrap_or_else(|error| error.into_inner());
    write_frame(&mut *stream, value)
}

/// Writes `{ beat: true }` every [`HEARTBEAT`] while a request runs, so a
/// live worker is never mistaken for a dead one. Dropping it stops the
/// beats and joins the thread at once.
struct Beats {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Beats {
    fn start(stream: Arc<Mutex<TcpStream>>) -> Self {
        let (stop, wait) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || loop {
            match wait.recv_timeout(HEARTBEAT) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let beat = SdkValue::map([("beat", SdkValue::Bool(true))]);
                    if write_locked(&stream, &beat).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}

impl Drop for Beats {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The operation was read and will run: distinct from the request being
/// sent and from a liveness beat.
fn claimed_frame(op: &str) -> SdkValue {
    SdkValue::map([
        ("claimed", SdkValue::Bool(true)),
        ("op", SdkValue::text(op)),
    ])
}

/// One operation-progress frame: `{ progress: { seq, op, phase, ... } }`.
fn progress_frame(seq: &mut u64, op: &str, phase: &str, more: Vec<(&str, SdkValue)>) -> SdkValue {
    *seq += 1;
    let mut fields: Vec<(String, SdkValue)> = vec![
        (
            "seq".to_owned(),
            SdkValue::Int(i64::try_from(*seq).unwrap_or(i64::MAX)),
        ),
        ("op".to_owned(), SdkValue::text(op)),
        ("phase".to_owned(), SdkValue::text(phase)),
    ];
    // the worker's own frame times the wait; it is not an operation event,
    // so a caller does not see it (the operation's own progress is)
    fields.push(("worker".to_owned(), SdkValue::Bool(true)));
    fields.extend(
        more.into_iter()
            .map(|(name, value)| (name.to_owned(), value)),
    );
    SdkValue::map([("progress", SdkValue::Map(fields))])
}

/// A progress value the operation path emitted, as a frame: its own fields
/// kept, with this worker's sequence and operation added.
fn progress_event(seq: &mut u64, op: &str, value: SdkValue) -> SdkValue {
    *seq += 1;
    let numbered = SdkValue::Int(i64::try_from(*seq).unwrap_or(i64::MAX));
    let value = match value {
        SdkValue::Map(_) => value,
        other => SdkValue::map([("report", other)]),
    };
    let value = crate::common::home::with_field(value, "seq", numbered);
    let value = crate::common::home::with_field(value, "op", SdkValue::text(op));
    SdkValue::map([("progress", value)])
}

fn ms(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn refusal_frame(refusal: SdkRefusal) -> SdkValue {
    SdkValue::map([(
        "refused",
        SdkValue::map([
            ("code", SdkValue::Text(refusal.code)),
            ("message", SdkValue::Text(refusal.message)),
            ("detail", refusal.detail),
        ]),
    )])
}

fn run(
    op: &str,
    args: SdkValue,
    root: &Path,
    events: &mut (dyn FnMut(SdkValue) + Send),
) -> Outcome {
    match op {
        "worker.ping" => Ok(SdkValue::map([
            ("kind", SdkValue::text("worker")),
            ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ])),
        // one Store call, on the Store file under this worker's root (2.0)
        "store.call" => {
            let name = args
                .get_text("op")
                .ok_or_else(|| refused("InvalidInput", "Store operation name"))?
                .to_owned();
            let inner = with_home(args.get("args").cloned().unwrap_or_default(), root);
            let store = inner
                .get_text("store")
                .ok_or_else(|| refused("InvalidInput", "a Store call names its Store"))?;
            crate::common::names::StoreName::parse(store)?;
            let file = crate::common::home::paths::store_file(root, store);
            // the worker holds the Store's lease under its root while it
            // keeps the Store open
            let lease = crate::common::home::lease::hold_locked(root, store)?;
            let inner = crate::common::home::with_field(
                inner,
                "storePath",
                SdkValue::text(file.display().to_string()),
            );
            crate::store::open::run(&name, inner, Some(lease))
        }
        // one step of a Store move or remove on this machine
        "store.file" => {
            let name = args
                .get_text("op")
                .ok_or_else(|| refused("InvalidInput", "Store step name"))?
                .to_owned();
            crate::store::transfer::steps::run(
                root,
                &name,
                &args.get("args").cloned().unwrap_or_default(),
            )
        }
        // a Sandbox or Workspace operation on this machine's own native
        "sandbox.host" => crate::sandbox::route::in_worker(root, &args, events),
        // drops one project from the Store under this worker's root (2.0):
        // `{ store, project, store_record }`
        "project.remove" => crate::store::identity::remove_project(&with_home(args, root)),
        // writes or discards one part of a caller's folder copy under this
        // worker's root (#536 H11a)
        "project.stage" => crate::project::staged::run(root, &args),
        // adds a folder copy the caller sent as a new project of a Store
        // under this worker's root (#536 H11, H11a)
        "project.init" => crate::project::init::in_worker(&with_home(args, root)),
        _ => Err(refused(
            "InvalidInput",
            format!("the worker has no operation {op}"),
        )),
    }
}

/// `args` with `home` set to the worker's root, whatever the caller sent.
/// A call that names a valid `store` gets that Store's file under the root
/// as its `storePath`, whatever the caller sent.
fn with_home(args: SdkValue, root: &Path) -> SdkValue {
    let file = args
        .get_text("store")
        .filter(|store| crate::common::names::StoreName::parse(store).is_ok())
        .map(|store| crate::common::home::paths::store_file(root, store));
    let text = SdkValue::text(root.display().to_string());
    let args = crate::common::home::with_field(args, "home", text);
    match file {
        Some(file) => crate::common::home::with_field(
            args,
            "storePath",
            SdkValue::text(file.display().to_string()),
        ),
        None => args,
    }
}
