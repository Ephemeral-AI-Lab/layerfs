//! The worker's side: `layerfs worker` serves one request per connection.
//! It runs every entity's operations, so it sits above them in
//! `layerfs-control` (layerfs-project#104); the protocol is
//! `layerfs-machine` `worker`.
//!
//! A connection starts with a small frame `{ secret }`: the worker reads at
//! most [`MAX_HELLO`](layerfs_machine::worker::MAX_HELLO) bytes, all of them
//! within one [`HELLO_WAIT`](layerfs_machine::worker::HELLO_WAIT), and answers
//! `{ ready: true, protocol, bundle }` ([`Hello`]), or refuses a wrong
//! secret and closes, before it reads anything larger. A caller of another
//! build sends nothing more and closes. Then a request `{ op, args }`, read with at most
//! [`REQUEST_WAIT`] between bytes. `worker.ping` answers who it is;
//! `inspect` reads this machine or one of its Sandboxes (`layerfs-sandbox`
//! `inspect`: at most two at once, each on a helper thread with deadlines);
//! `store.call`, `store.file`, `store.compact`, `store.reach`, `sandbox.host`,
//! `project.stage`, `project.init`, `project.export`, `project.delete` and
//! `project.list` run one operation on this machine (`ops`),
//! with `home` set to the worker's own root (the machine's data disk),
//! never a path the caller sends. `store.compact` runs a sqlite Store's
//! compaction where its file is (spec S6, #42 D5). Answers are
//! `{ value }` or `{ refused }`, after any `{ event }` frames.
//!
//! A request may carry `home_id`, the caller's home id: the root is not a
//! home and has no id of its own (O33), so host code that asks for one
//! gets the caller's, and the root is never converted or written as a home.
//!
//! A request with `trace_step: true` (a caller with `LAYERFS_TRACE_STEP`
//! set) has its steps timed from when it was read, and its answer carries
//! them as `steps: [{ step, us }]` beside `value` or `refused`.
//!
//! One connection never stops or holds up the worker (layerfs-project#177).
//! This file is the accept loop: a failed `accept` that says nothing about
//! the listener, and a thread the OS cannot start, are logged and passed
//! over. `connection` bounds what one peer can hold: the whole hello by
//! one deadline, each send by [`REQUEST_WAIT`] without progress, and a
//! panic in an operation by that connection alone.
mod connection;
#[cfg(feature = "worker-fault")]
pub mod fault;
mod ops;

pub use connection::REQUEST_WAIT;
use layerfs_bridge::control::SdkValue;
use layerfs_machine::worker::{Hello, HELLO_WAIT};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What a worker served before it stopped.
#[derive(Debug, Default)]
pub struct Served {
    /// Requests answered.
    pub requests: u64,
}

/// How long the worker waits, and how often it logs one kind of failure.
#[derive(Clone, Copy, Debug)]
struct Waits {
    /// The whole hello: every byte of the secret's frame.
    hello: Duration,
    /// The next byte of a request.
    request: Duration,
    /// One send that makes no progress.
    write: Duration,
    /// The caller's close, once it has its answer.
    close: Duration,
    /// Between a failed `accept` and the next.
    accept_pause: Duration,
    /// Between two log lines of one kind of accept or thread failure.
    log_every: Duration,
}

impl Waits {
    const SHIPPED: Self = Self {
        hello: HELLO_WAIT,
        request: REQUEST_WAIT,
        write: REQUEST_WAIT,
        close: REQUEST_WAIT,
        accept_pause: Duration::from_millis(50),
        log_every: Duration::from_secs(10),
    };
}

/// One worker: what every connection of it reads.
struct Server {
    secret: String,
    /// Its own state root.
    root: PathBuf,
    /// What it answers every secret with, read once.
    ready: SdkValue,
    /// When it began to serve, in milliseconds since the epoch.
    started: i64,
    /// This worker process's incarnation: one fresh token per start, which
    /// the mounts it places carry and the host's start reconcile records.
    incarnation: String,
    waits: Waits,
    #[cfg(feature = "worker-fault")]
    faults: Arc<fault::Faults>,
}

/// One fresh incarnation for a worker no host supplied one to (an older
/// host, or a test). The worker knows no home, so the value carries no home
/// id and orders against nothing: it is only reported, and a host whose
/// record names another value skips its reconcile.
fn incarnation() -> String {
    layerfs_home::helpers::ordered_token("", "worker", &[])
}

/// Serves `listener` until it fails or the process is ended (`machine.stop`;
/// there is no idle stop, #535 D11). `root` is the worker's own state root.
/// It returns only when the listener itself is unusable.
pub fn serve(listener: TcpListener, secret: String, root: PathBuf) -> std::io::Result<Served> {
    serve_with_incarnation(listener, secret, root, None)
}

/// Serves with the ordered incarnation the host's machine start persisted
/// before launching this process, or mints one when the caller supplies none
/// (an older host, or a test). Either way the value is durable under `root`
/// before the first request is served: every mount intent this worker places
/// carries it, and the host's reconcile writes the same value into each Store
/// and settles what the previous one left. A write that fails ends the start;
/// it never serves with an incarnation it cannot record.
pub fn serve_with_incarnation(
    listener: TcpListener,
    secret: String,
    root: PathBuf,
    supplied: Option<String>,
) -> std::io::Result<Served> {
    let server = Server::new(secret, root, supplied);
    server.run(&listener)
}

/// Prepares this machine for its Sandboxes' own networks as its worker
/// starts listening on `address`: a non-loopback address is closed to
/// every Sandbox's commands, and on Linux the firewall table is written
/// once for the machine's pool (each Sandbox start writes it again). The
/// line the worker logs; a table that cannot be written is logged, not
/// fatal: each Sandbox start then refuses with its own reason.
pub fn prepare_network(address: std::net::SocketAddr) -> String {
    layerfs_sandbox::launch::network::serving_on(address);
    network_table()
}

#[cfg(target_os = "linux")]
fn network_table() -> String {
    match layerfs_sandbox::launch::network::worker_start() {
        Ok(pool) => format!("Sandbox networks: table inet layerfs written for pool {pool}"),
        Err(why) => format!("Sandbox networks: the table was not written: {why}"),
    }
}

#[cfg(not(target_os = "linux"))]
fn network_table() -> String {
    "Sandbox networks: none on this OS".to_owned()
}

/// [`serve`] with the switches and waits of `faults`.
#[cfg(feature = "worker-fault")]
pub fn serve_with(
    listener: TcpListener,
    secret: String,
    root: PathBuf,
    faults: Arc<fault::Faults>,
) -> std::io::Result<Served> {
    let mut server = Server::new(secret, root, None);
    server.waits = faults.waits();
    server.faults = faults;
    server.run(&listener)
}

/// The name of an `accept` failure that says nothing about the listener:
/// the process or the system is out of descriptors, buffers or memory, or
/// the peer gave up before it was accepted. `None` for every other failure
/// (`EBADF`, `EINVAL`, `ENOTSOCK`: the listener is not one), which ends
/// [`serve`].
fn passing(error: &std::io::Error) -> Option<&'static str> {
    match error.raw_os_error()? {
        libc::EMFILE => Some("EMFILE"),
        libc::ENFILE => Some("ENFILE"),
        libc::ENOBUFS => Some("ENOBUFS"),
        libc::ENOMEM => Some("ENOMEM"),
        libc::ECONNABORTED => Some("ECONNABORTED"),
        libc::EINTR => Some("EINTR"),
        _ => None,
    }
}

/// When each kind of failure was last logged, and how many of it were not.
struct Limited {
    every: Duration,
    kinds: Vec<(&'static str, Instant, u64)>,
}

impl Limited {
    /// Whether a failure of `kind` is logged now: at most one line of a
    /// kind in each `every`. `Some` holds how many of that kind went
    /// unlogged since its last line.
    fn due(&mut self, kind: &'static str) -> Option<u64> {
        let now = Instant::now();
        match self.kinds.iter_mut().find(|(known, ..)| *known == kind) {
            None => {
                self.kinds.push((kind, now, 0));
                Some(0)
            }
            Some((_, at, unlogged)) if now.duration_since(*at) >= self.every => {
                *at = now;
                Some(std::mem::take(unlogged))
            }
            Some((_, _, unlogged)) => {
                *unlogged += 1;
                None
            }
        }
    }
}

impl Server {
    fn new(secret: String, root: PathBuf, supplied: Option<String>) -> Self {
        Self {
            secret,
            root,
            ready: Hello::own().to_frame(),
            started: layerfs_home::home::now_ms(),
            incarnation: supplied
                .filter(|value| !value.is_empty())
                .unwrap_or_else(incarnation),
            waits: Waits::SHIPPED,
            #[cfg(feature = "worker-fault")]
            faults: Arc::default(),
        }
    }

    /// One line of the worker's log (its stderr, `worker.log`). It never
    /// holds a request's arguments or a secret.
    fn log(&self, line: &str) {
        let _ = writeln!(std::io::stderr(), "layerfs worker: {line}");
        #[cfg(feature = "worker-fault")]
        self.faults.logged(line);
    }

    fn accept(&self, listener: &TcpListener) -> std::io::Result<TcpStream> {
        #[cfg(feature = "worker-fault")]
        if let Some(error) = self.faults.accept_error() {
            return Err(error);
        }
        listener.accept().map(|(stream, _)| stream)
    }

    /// Serves `stream` on a thread of its own. When the OS cannot start
    /// one, the stream is dropped: its caller sees the connection close
    /// before the hello, and nothing was sent.
    fn spawn(self: &Arc<Self>, stream: TcpStream) -> std::io::Result<()> {
        #[cfg(feature = "worker-fault")]
        if let Some(error) = self.faults.spawn_error() {
            return Err(error);
        }
        let server = self.clone();
        std::thread::Builder::new()
            .name("worker-request".to_owned())
            .spawn(move || connection::serve(&server, stream))
            .map(drop)
    }

    fn run(self, listener: &TcpListener) -> std::io::Result<Served> {
        // the root holds Stores and records: private, as a home is (a machine's
        // data disk mounts 0755), but passable by others (0701), since a
        // namespaces Sandbox's commands run as another identity below it
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o701)
                .create(&self.root)?;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o701))?;
        }
        // Before the first request: an incarnation the worker cannot record
        // is a start that never serves.
        layerfs_home::home::write_entity(
            &self.root.join("worker-incarnation.json"),
            &SdkValue::map([("incarnation", SdkValue::text(&self.incarnation))]),
        )
        .map_err(|refusal| {
            std::io::Error::other(format!(
                "the worker cannot record its incarnation: {}",
                refusal.message
            ))
        })?;
        layerfs_home::home::guest::serve_root(&self.root);
        // what earlier Sandbox starts and removals left in this machine's
        // run root goes before the first request (spec v20), inside its
        // bound, after the records of daemons that ended (the machine
        // stopped); its outcome is in `log/sweep.jsonl`
        layerfs_sandbox::sweep::worker_start(&self.root);
        let server = Arc::new(self);
        let mut limited = Limited {
            every: server.waits.log_every,
            kinds: Vec::new(),
        };
        let unlogged = |more: u64| match more {
            0 => String::new(),
            more => format!("; {more} more since this was last logged"),
        };
        loop {
            let stream = match server.accept(listener) {
                Ok(stream) => stream,
                Err(error) => {
                    let Some(kind) = passing(&error) else {
                        server.log(&format!(
                            "accept failed ({error}); the listener is unusable, the worker stops"
                        ));
                        return Err(error);
                    };
                    if let Some(more) = limited.due(kind) {
                        server.log(&format!(
                            "accept failed ({kind}: {error}); still serving{}",
                            unlogged(more)
                        ));
                    }
                    // a pause, not a retry: nothing was received or is resent
                    std::thread::sleep(server.waits.accept_pause);
                    continue;
                }
            };
            if let Err(error) = server.spawn(stream) {
                if let Some(more) = limited.due("thread") {
                    server.log(&format!(
                        "no thread for a connection ({error}); it was closed unanswered, still serving{}",
                        unlogged(more)
                    ));
                }
            }
        }
    }
}
