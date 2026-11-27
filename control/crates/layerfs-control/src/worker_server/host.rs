//! Host worker on the real home, with separate admission for management
//! and for streamed writes.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_machine::worker::{
    record_worker_start_failure, HostHello, HostPaths, HOST_HELD_WRITES,
};
use nix::poll::{poll, PollFd, PollFlags};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

mod postgres;
mod write;
pub(super) use write::Held;

/// Whether this worker was built with its test fault switches.
#[cfg(feature = "worker-fault")]
const FAULTS: bool = true;
/// Whether this worker was built with its test fault switches.
#[cfg(not(feature = "worker-fault"))]
const FAULTS: bool = false;

const IDLE: Duration = Duration::from_secs(60);
const IDLE_CHECK: Duration = Duration::from_secs(5);
/// How long an idle worker keeps listening after it removed its socket
/// names, so a `connect` that already resolved a name lands in a listen
/// queue that is still read.
const UNPUBLISHED: Duration = Duration::from_secs(1);
const ORDINARY: Lane = Lane {
    index: 0,
    slots: 16,
    threads: 8,
    name: "layerfs-host-call",
};
const CONTROL: Lane = Lane {
    index: 1,
    slots: 2,
    threads: 2,
    name: "layerfs-host-control",
};
/// Streamed writes' own endpoint (S2.2): its threads read each hello and
/// request and hold the write for a write thread. Its listener accepts only
/// while its connections and held writes together are below
/// [`HOST_HELD_WRITES`], so further writers wait in its listen queue and,
/// past their hello bound, are refused before they send anything (§11).
const WRITES: Lane = Lane {
    index: 3,
    slots: HOST_HELD_WRITES,
    threads: 2,
    name: "layerfs-host-intake",
};
/// PostgreSQL reads and single-change writes (#819 P-B), handed over by an
/// ordinary thread once their request is read, so they never occupy an
/// ordinary thread or slot while they wait or run: calls on other Stores
/// keep progressing. A lane thread starts the oldest queued call whose
/// Store has a free session slot of its kind (`pool::reserve_kind`), so a
/// call of a busy Store waits in the queue, on no thread, and never holds a
/// thread another Store could use (`postgres.rs`). `slots` bounds the calls
/// handed over and not yet answered; at it the ordinary thread answers the
/// call `Unavailable` at once, unrun, and never waits on this lane. Calls
/// whose caller has gone are dropped unrun.
const POSTGRES: Lane = Lane {
    index: 2,
    slots: 64,
    threads: layerfs_store::postgresql::pool::PER_PROCESS,
    name: "layerfs-host-postgresql",
};

#[derive(Clone, Copy)]
struct Lane {
    index: usize,
    slots: usize,
    threads: usize,
    name: &'static str,
}

struct Pending {
    stream: UnixStream,
    /// A PostgreSQL read's request, already read by an ordinary thread.
    request: Option<SdkValue>,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<Pending>,
    /// Accepted sockets in this lane, including the queue and all exchanges.
    outstanding: usize,
}

struct Activity {
    lanes: [Queue; 4],
    last_call: Instant,
    admitted: usize,
    draining: bool,
    answered: u64,
    last_close: Option<SdkValue>,
    writes: write::Writes,
}

impl Activity {
    fn new() -> Self {
        Self {
            lanes: Default::default(),
            last_call: Instant::now(),
            admitted: 0,
            draining: false,
            answered: 0,
            last_close: None,
            writes: write::Writes::default(),
        }
    }

    /// Whether `lane`'s listener may accept: below its slots and, for the
    /// write endpoint, with the held writes counted.
    fn accepts(&self, lane: Lane) -> bool {
        let mut outstanding = self.lanes[lane.index].outstanding;
        if lane.index == WRITES.index {
            outstanding += self.writes.held();
        }
        outstanding < lane.slots
    }

    /// Nothing in flight, counting connections accepted but not admitted
    /// and, as one of `lanes`, the PostgreSQL reads handed over and not yet
    /// answered: a queued or running read keeps the worker from idle exit.
    fn quiet(&self) -> bool {
        self.admitted == 0
            && self.lanes.iter().all(|lane| lane.outstanding == 0)
            && self.writes.held() == 0
    }
}

pub(super) struct HostServer {
    pub(super) hello: HostHello,
    home: PathBuf,
    home_id: String,
    identity: Vec<(&'static str, SdkValue)>,
    activity: Mutex<Activity>,
    queued: [Condvar; 4],
    /// Wakes a write thread: a write is held, or the worker drains.
    writable: Condvar,
    wake: UnixStream,
}

impl HostServer {
    fn lock(&self) -> MutexGuard<'_, Activity> {
        self.activity
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn notify(&self) {
        // A full nonblocking wake socket already has a readable notification.
        let _ = (&self.wake).write(&[1]);
    }

    fn drain(&self, activity: &mut Activity) {
        activity.draining = true;
        for ready in &self.queued {
            ready.notify_all();
        }
        self.writable.notify_all();
        self.notify();
    }

    fn pop(&self, lane: Lane) -> Option<Pending> {
        let mut activity = self.lock();
        loop {
            if let Some(connection) = activity.lanes[lane.index].pending.pop_front() {
                return Some(connection);
            }
            if activity.draining {
                return None;
            }
            activity = self.queued[lane.index]
                .wait(activity)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn work(&self, lane: Lane) {
        while let Some(Pending { stream, request }) = self.pop(lane) {
            let mut admitted = false;
            let serve = |stream: &UnixStream, request, admitted: &mut bool| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    super::connection::serve_host_connection(
                        self,
                        stream,
                        endpoint(lane),
                        request,
                        admitted,
                    )
                }))
            };
            let (stream, mut result) = match serve(&stream, request, &mut admitted) {
                // a PostgreSQL call goes to its own lane; draining, it is
                // answered here (refused, as every call admitted then)
                Ok(Served::Postgres(request)) => {
                    let serve =
                        |open: &UnixStream, request| serve(open, Some(request), &mut admitted);
                    self.handed(stream, request, serve)
                }
                other => (Some(stream), other),
            };
            // a write is held before this slot is released, so the worker
            // is never quiet in between
            result = match result {
                Ok(Served::Write(held)) => {
                    self.hold_write(held);
                    Ok(Served::HandedOver)
                }
                other => other,
            };
            if result.is_err() {
                let _ = layerfs_home::trace::steps_take();
                log("a host connection panicked; its answer was lost");
            }
            drop(stream);
            self.finish(lane, admitted, &result);
        }
    }

    /// One connection of `lane` ended, with `result`.
    fn finish(&self, lane: Lane, admitted: bool, result: &std::thread::Result<Served>) {
        {
            let mut activity = self.lock();
            activity.lanes[lane.index].outstanding -= 1;
            if admitted {
                activity.admitted -= 1;
                // Only calls keep the worker warm (§8.3, §7.3). Management
                // observes it: polling ping must not keep it alive (§12).
                if lane.index != CONTROL.index {
                    activity.last_call = Instant::now();
                }
            }
            if matches!(result, Ok(Served::Answered(true))) {
                activity.answered += 1;
            }
        }
        if lane.index == POSTGRES.index {
            // a session slot is free again
            self.slot_freed();
        }
        self.notify();
    }

    fn accept(&self, listener: &UnixListener, lane: Lane) -> io::Result<()> {
        // Only this thread accepts. A worker can free a slot during poll,
        // but it cannot consume one, so readiness never overbooks the lane.
        let mut activity = self.lock();
        if activity.draining {
            return Ok(());
        }
        if !activity.accepts(lane) {
            return Ok(());
        }
        let queue = &mut activity.lanes[lane.index];
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                queue.outstanding += 1;
                queue.pending.push_back(Pending {
                    stream,
                    request: None,
                });
                self.queued[lane.index].notify_one();
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn listen(
        &self,
        [ordinary, writes, control]: [&UnixListener; 3],
        mut wake_read: UnixStream,
        unpublish: &dyn Fn(),
    ) -> io::Result<()> {
        let mut limited = super::Limited {
            every: Duration::from_secs(10),
            kinds: Vec::new(),
        };
        // §8.3 idle exit: first remove the socket names, so no new caller can
        // join a listen queue; keep serving whatever is already queued; drain
        // only once quiet with every listen queue empty. A caller after the removal
        // finds no endpoint and starts the next worker once this one exits.
        let mut unpublished: Option<Instant> = None;
        loop {
            let mut activity = self.lock();
            if activity.draining {
                return Ok(());
            }
            if activity.quiet() {
                match unpublished {
                    None if activity.last_call.elapsed() >= IDLE => {
                        drop(activity);
                        unpublish();
                        unpublished = Some(Instant::now());
                        continue;
                    }
                    Some(at)
                        if at.elapsed() >= UNPUBLISHED
                            && !queued(ordinary)?
                            && !queued(writes)?
                            && !queued(control)? =>
                    {
                        self.drain(&mut activity);
                        return Ok(());
                    }
                    _ => {}
                }
            }
            // a full write endpoint accepts again once a waiting writer
            // that went away is dropped: checked here, at least every
            // `IDLE_CHECK`
            if !activity.accepts(WRITES) {
                activity.writes.purge_gone();
            }
            let interests = |lane: Lane| {
                if activity.accepts(lane) {
                    PollFlags::POLLIN
                } else {
                    PollFlags::empty()
                }
            };
            let mut ready = [
                PollFd::new(wake_read.as_fd(), PollFlags::POLLIN),
                PollFd::new(control.as_fd(), interests(CONTROL)),
                PollFd::new(ordinary.as_fd(), interests(ORDINARY)),
                PollFd::new(writes.as_fd(), interests(WRITES)),
            ];
            drop(activity);
            let check = if unpublished.is_some() {
                UNPUBLISHED
            } else {
                IDLE_CHECK
            };
            match poll(&mut ready, check.as_millis() as u16) {
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(error) => return Err(error.into()),
            }
            let events = ready.map(|fd| fd.revents().unwrap_or_else(PollFlags::empty));
            if events[0].contains(PollFlags::POLLIN) {
                let _ = wake_read.read(&mut [0; 64]);
            }
            for (events, listener, lane) in [
                (events[1], control, CONTROL),
                (events[2], ordinary, ORDINARY),
                (events[3], writes, WRITES),
            ] {
                if events.intersects(PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL)
                {
                    return Err(io::Error::other("the host worker listener failed"));
                }
                if events.contains(PollFlags::POLLIN) {
                    if let Err(error) = self.accept(listener, lane) {
                        let Some(kind) = super::passing(&error) else {
                            return Err(error);
                        };
                        if limited.due(kind).is_some() {
                            log(&format!("accept failed ({kind}): {error}"));
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        }
    }

    /// The one admission step of every call and write: refused before any
    /// effect while draining, otherwise counted as admitted under the
    /// returned lock.
    fn admit(&self, admitted: &mut bool) -> Result<MutexGuard<'_, Activity>, SdkRefusal> {
        let mut activity = self.lock();
        if activity.draining {
            let mut error = refused("Unavailable", "the host worker is draining");
            error.detail = SdkValue::map([("state", SdkValue::text("draining"))]);
            return Err(error);
        }
        activity.admitted += 1;
        *admitted = true;
        Ok(activity)
    }

    pub(super) fn dispatch(
        &self,
        control: bool,
        op: &str,
        args: &SdkValue,
        admitted: &mut bool,
    ) -> Outcome {
        // Admission and drain share one lock. Store work later runs after
        // admission, outside this lock, on the ordinary pool. Drain needs no
        // admission: it is answered while draining too.
        if control && op == "drain" {
            return self.drain_answer();
        }
        let stores =
            (control && op == "ping").then(|| layerfs_store::open::host_store_count(&self.home));
        enum Operation {
            Ping,
            CloseStore,
            StoreCall,
            PostgresRead,
            PostgresRetire,
        }
        let operation = match (control, op) {
            (true, "ping") => Operation::Ping,
            (true, layerfs_store::host_maintenance::CLOSE) => Operation::CloseStore,
            (false, layerfs_store::host_read::OP) => Operation::StoreCall,
            (false, layerfs_store::host_postgresql::OP) => Operation::PostgresRead,
            (true, layerfs_store::host_postgresql::RETIRE) => Operation::PostgresRetire,
            _ => {
                return Err(refused(
                    "Unsupported",
                    "the operation is unavailable on this host worker endpoint",
                ))
            }
        };
        let activity = self.admit(admitted)?;
        match operation {
            Operation::Ping => {
                let identity = self.identity.iter().cloned();
                let fields = [
                    ("state", SdkValue::text("ready")),
                    ("generation", SdkValue::text(&self.hello.generation)),
                    ("home_id", SdkValue::text(&self.home_id)),
                    ("build", SdkValue::text(&self.hello.build)),
                    (
                        "ordinary",
                        SdkValue::Int(activity.lanes[ORDINARY.index].outstanding as i64),
                    ),
                    (
                        "control",
                        SdkValue::Int(activity.lanes[CONTROL.index].outstanding as i64),
                    ),
                    ("admitted", SdkValue::Int(activity.admitted as i64)),
                    ("writes", SdkValue::Int(activity.writes.counts().0 as i64)),
                    (
                        "writes_waiting",
                        SdkValue::Int(activity.writes.counts().1 as i64),
                    ),
                    (
                        "unrecorded",
                        SdkValue::Int(activity.writes.unrecorded_count() as i64),
                    ),
                    ("faults", SdkValue::Bool(FAULTS)),
                    ("stores", SdkValue::Int(stores.unwrap_or_default() as i64)),
                    ("postgresql", postgres::ping(&activity)),
                    (
                        "queued",
                        SdkValue::Int(
                            activity
                                .lanes
                                .iter()
                                .map(|lane| lane.pending.len() as i64)
                                .sum(),
                        ),
                    ),
                    (
                        "last_close",
                        activity.last_close.clone().unwrap_or(SdkValue::Null),
                    ),
                    (
                        "requests",
                        SdkValue::Int(i64::try_from(activity.answered).unwrap_or(i64::MAX)),
                    ),
                ];
                Ok(SdkValue::Map(
                    identity
                        .chain(fields)
                        .map(|(key, value)| (key.to_owned(), value))
                        .collect(),
                ))
            }
            Operation::CloseStore => {
                drop(activity);
                self.close_store(args)
            }
            Operation::StoreCall => {
                drop(activity);
                layerfs_store::host_read::serve(&self.home, args)
            }
            Operation::PostgresRead => {
                drop(activity);
                layerfs_store::host_postgresql::serve(&self.home, args)
            }
            Operation::PostgresRetire => {
                drop(activity);
                layerfs_store::host_postgresql::retire_request(&self.home, args)
            }
        }
    }

    /// Starts the drain, or answers one already started, with the same
    /// account: uncertain write outcomes no record holds, up to now (§8.5),
    /// and the admitted writes still running, which drain lets finish (a
    /// write still waiting for a thread is refused unrun).
    fn drain_answer(&self) -> Outcome {
        let mut activity = self.lock();
        self.drain(&mut activity);
        let [listed, count] = activity.writes.unrecorded();
        let held = activity.writes.counts().0;
        drop(activity);
        self.close_drained_stores();
        Ok(SdkValue::map([
            ("state", SdkValue::text("draining")),
            listed,
            count,
            ("writes_running", SdkValue::Int(held as i64)),
        ]))
    }

    fn close_store(&self, args: &SdkValue) -> Outcome {
        let report =
            layerfs_store::host_maintenance::close_at(&self.home, &self.hello.generation, args);
        self.record_close(
            args.get_text("store").unwrap_or_default(),
            "maintenance",
            &report,
        );
        report.map(|report| {
            SdkValue::map([
                ("closed", SdkValue::Bool(report.closed)),
                (
                    "wal_remaining",
                    report.wal_remaining.map_or(SdkValue::Null, SdkValue::Bool),
                ),
            ])
        })
    }

    fn record_close(
        &self,
        store: &str,
        reason: &str,
        report: &Result<layerfs_store::open::CloseReport, SdkRefusal>,
    ) {
        // Do not put an arbitrary request value into the worker log or ping.
        if layerfs_home::names::StoreName::parse(store).is_err() {
            return;
        }
        let (status, closed, wal) = match report {
            Ok(report) => {
                let status = match &report.checkpoint {
                    Ok(Some(true)) => "checkpointed",
                    Ok(Some(false)) => "checkpoint-deferred",
                    Ok(None) => "no-checkpoint",
                    Err(_) => "checkpoint-failed",
                };
                (status, SdkValue::Bool(report.closed), report.wal_remaining)
            }
            Err(error) => (error.code.as_str(), SdkValue::Null, None),
        };
        let summary = SdkValue::map([
            ("store", SdkValue::text(store)),
            ("at", SdkValue::Int(layerfs_home::home::now_ms())),
            ("status", SdkValue::text(status)),
            ("closed", closed),
            ("wal_remaining", wal.map_or(SdkValue::Null, SdkValue::Bool)),
            ("generation", SdkValue::text(&self.hello.generation)),
            ("reason", SdkValue::text(reason)),
        ]);
        // The home logger opens and closes its file per record and rotates
        // it under its own log lock. No worker admission lock is held here.
        if let Err(error) = layerfs_home::home::log_line(&self.home, "worker-close.jsonl", &summary)
        {
            log(&format!(
                "could not record Store {store}'s close outcome: {}",
                error.code
            ));
        }
        self.lock().last_close = Some(summary);
    }

    fn close_drained_stores(&self) {
        for (path, report) in layerfs_store::open::close_idle_report(Duration::ZERO) {
            if let Some(store) = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
            {
                self.record_close(store, "drain", &report);
            }
        }
    }
}

/// Whether the caller of `stream` has closed or reset its end: a hang-up,
/// an error, or end of file seen by a peek that consumes nothing. Bytes the
/// caller sends are not a close. A held write's caller sends nothing after
/// its request until its `go` and never shuts down only its write side
/// (which macOS `poll` may report as `POLLHUP`), so this serves both the
/// queued PostgreSQL reads and the writes waiting for a write thread.
fn peer_gone(stream: &UnixStream) -> bool {
    use nix::sys::socket::{recv, MsgFlags};
    let mut ready = [PollFd::new(stream.as_fd(), PollFlags::POLLIN)];
    let Ok(_) = poll(&mut ready, 0u16) else {
        return false;
    };
    let Some(events) = ready[0].revents() else {
        return false;
    };
    if events.intersects(PollFlags::POLLHUP | PollFlags::POLLERR) {
        return true;
    }
    if !events.contains(PollFlags::POLLIN) {
        return false;
    }
    let mut byte = [0u8; 1];
    let peeked = recv(
        stream.as_raw_fd(),
        &mut byte,
        MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
    );
    match peeked {
        Ok(0) => true,
        Ok(_) => false,
        Err(nix::errno::Errno::EAGAIN | nix::errno::Errno::EINTR) => false,
        Err(_) => true,
    }
}

/// Which endpoint a connection came in on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Endpoint {
    Ordinary,
    Control,
    /// Streamed writes only (S2.2).
    Writes,
}

fn endpoint(lane: Lane) -> Endpoint {
    match lane.index {
        index if index == CONTROL.index => Endpoint::Control,
        index if index == WRITES.index => Endpoint::Writes,
        _ => Endpoint::Ordinary,
    }
}

/// What one connection came to on its thread.
pub(super) enum Served {
    /// Answered (`true`) or ended without an answer.
    Answered(bool),
    /// A PostgreSQL read whose request was read, for its own lane.
    Postgres(SdkValue),
    /// Handed to the PostgreSQL lane or the write lane, which answers and
    /// accounts it.
    HandedOver,
    /// A streamed `store.write` read on the write endpoint, held for a write
    /// thread with its connection.
    Write(Held),
}

/// Whether a connection is waiting in the listener's kernel queue.
fn queued(listener: &UnixListener) -> io::Result<bool> {
    let mut ready = [PollFd::new(listener.as_fd(), PollFlags::POLLIN)];
    match poll(&mut ready, 0u16) {
        Ok(_) => Ok(ready[0]
            .revents()
            .is_some_and(|events| events.contains(PollFlags::POLLIN))),
        Err(nix::errno::Errno::EINTR) => Ok(true),
        Err(error) => Err(error.into()),
    }
}

fn log(message: &str) {
    let _ = writeln!(io::stderr(), "layerfs host worker: {message}");
}

fn run(
    listeners: [UnixListener; 3],
    hello: HostHello,
    home: PathBuf,
    home_id: String,
    unpublish: &dyn Fn(),
) -> io::Result<super::Served> {
    for listener in &listeners {
        listener.set_nonblocking(true)?;
    }
    let (wake_read, wake) = UnixStream::pair()?;
    wake_read.set_nonblocking(true)?;
    wake.set_nonblocking(true)?;
    let server = HostServer {
        hello,
        home,
        home_id,
        identity: layerfs_home::process::identity(),
        activity: Mutex::new(Activity::new()),
        queued: Default::default(),
        writable: Condvar::new(),
        wake,
    };
    let served = std::thread::scope(|scope| {
        let result = (|| {
            let server = &server;
            for lane in [CONTROL, ORDINARY, WRITES, POSTGRES] {
                for _ in 0..lane.threads {
                    std::thread::Builder::new()
                        .name(lane.name.to_owned())
                        .spawn_scoped(scope, move || match lane.index == POSTGRES.index {
                            true => server.work_postgres(),
                            false => server.work(lane),
                        })?;
                }
            }
            for _ in 0..write::WRITE_THREADS {
                std::thread::Builder::new()
                    .name("layerfs-host-write".to_owned())
                    .spawn_scoped(scope, move || server.work_writes())?;
            }
            let [ordinary, writes, control] = &listeners;
            server.listen([ordinary, writes, control], wake_read, unpublish)
        })();
        server.drain(&mut server.lock());
        drop(listeners);
        // Scope exit joins accepted connections before releasing the owner.
        result
    });
    server.close_drained_stores();
    served?;
    let requests = server.lock().answered;
    Ok(super::Served { requests })
}

/// Points stdin at `/dev/null` once every endpoint is published: a caller
/// that started this worker in the background handed it the startup lock
/// as its stdin (#833), and other callers may now start a worker again
/// (which finds this one ready). Any other stdin is dropped the same way.
fn release_startup_lock() {
    let released = std::fs::File::open("/dev/null")
        .and_then(|null| nix::unistd::dup2_stdin(&null).map_err(io::Error::from));
    if let Err(error) = released {
        log(&format!("startup lock kept until exit: {error}"));
    }
}

/// Records a panic before this worker publishes its endpoints as its failed
/// start (#833), for doctor, then panics as before; the flag it answers is
/// set once they are published, after which a panic is not a start's.
fn record_startup_panics(home: &Path) -> Arc<AtomicBool> {
    let published = Arc::new(AtomicBool::new(false));
    let (flag, home) = (published.clone(), home.to_owned());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !flag.load(Ordering::Acquire) {
            record_worker_start_failure(&home, &format!("the worker {info}"));
        }
        previous(info);
    }));
    published
}

/// Runs the host worker with the real home and its lifetime lock.
/// This entry runs in its own process, above the Store and machine crates.
pub fn serve_host(home: &Path) -> Result<super::Served, SdkRefusal> {
    // before any request, which may carry a PostgreSQL password, is read
    layerfs_store::host_postgresql::limit_core_dumps();
    let started = record_startup_panics(home);
    let paths = HostPaths::new(home)
        .inspect_err(|error| record_worker_start_failure(home, &error.message))?;
    let guard = paths
        .try_hold()
        .inspect_err(|error| paths.record_start_failure(&error.message))?
        .ok_or_else(|| {
            refused(
                "Unavailable",
                "another host worker owns the endpoint for this home",
            )
        })?;
    let run = (|| {
        let published = (|| {
            let hello = HostHello::own(&paths)?;
            let _home = layerfs_home::home::lock::take(home)?;
            layerfs_machine::record::MachineRecord::admit_host(
                home,
                layerfs_home::names::HOST_MACHINE,
            )?;
            Ok::<_, SdkRefusal>((
                hello,
                [
                    paths.bind(false, &guard)?,
                    paths.bind_writes(&guard)?,
                    paths.bind(true, &guard)?,
                ],
            ))
        })();
        let (hello, listeners) = published.inspect_err(|error| {
            // a background start's caller has gone: doctor reads it here
            paths.record_start_failure(&error.message);
        })?;
        paths.clear_start_failure();
        started.store(true, Ordering::Release);
        release_startup_lock();
        run(
            listeners,
            hello,
            paths.home.clone(),
            paths.home_id.clone(),
            &|| {
                if let Err(error) = paths.remove_endpoints(&guard) {
                    log(&format!("idle socket removal failed: {}", error.message));
                }
            },
        )
        .map_err(failed)
    })();
    let cleanup = paths.remove_endpoints(&guard);
    if let Err(error) = &cleanup {
        log(&format!("socket cleanup failed: {}", error.message));
    }
    run.and_then(|served| cleanup.map(|()| served))
}
