//! Host worker on the real home, with separate admission for management.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_machine::worker::{HostHello, HostPaths};
use nix::poll::{poll, PollFd, PollFlags};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

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
/// PostgreSQL reads (#819 P-B), handed over by an ordinary thread once their
/// request is read, so they never occupy an ordinary thread or slot while
/// they wait: calls on other Stores keep progressing. A lane thread starts
/// the oldest queued read whose Store has a free session slot
/// (`pool::reserve`), so a read of a busy Store waits in the queue, on no
/// thread, and never holds a thread another Store could use. `slots` bounds
/// the reads handed over and not yet answered; at it the ordinary thread
/// answers the read `Unavailable` at once, unrun, and never waits on this
/// lane. Reads whose caller has gone are dropped unrun.
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
    lanes: [Queue; 3],
    last_call: Instant,
    admitted: usize,
    draining: bool,
    answered: u64,
    last_close: Option<SdkValue>,
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
        }
    }

    /// Nothing in flight, counting connections accepted but not admitted
    /// and, as one of `lanes`, the PostgreSQL reads handed over and not yet
    /// answered: a queued or running read keeps the worker from idle exit.
    fn quiet(&self) -> bool {
        self.admitted == 0 && self.lanes.iter().all(|lane| lane.outstanding == 0)
    }
}

pub(super) struct HostServer {
    pub(super) hello: HostHello,
    home: PathBuf,
    home_id: String,
    identity: Vec<(&'static str, SdkValue)>,
    activity: Mutex<Activity>,
    queued: [Condvar; 3],
    wake: UnixStream,
}

impl HostServer {
    fn lock(&self) -> std::sync::MutexGuard<'_, Activity> {
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
                        lane.index == CONTROL.index,
                        request,
                        admitted,
                    )
                }))
            };
            let mut result = serve(&stream, request, &mut admitted);
            let mut stream = Some(stream);
            if let Ok(Served::Postgres(request)) = result {
                // a PostgreSQL read goes to its own lane; draining, it is
                // answered here (refused, as every call admitted then)
                result = match stream.take().map(|open| self.hand_over(open, request)) {
                    Some(HandOver::Back(open, request)) => {
                        let served = serve(&open, Some(request), &mut admitted);
                        stream = Some(open);
                        served
                    }
                    Some(HandOver::Full(open, request)) => {
                        let refusal = refused(
                            "Unavailable",
                            format!(
                                "the host worker holds {} PostgreSQL reads not yet \
                                 answered; this read was not run",
                                POSTGRES.slots
                            ),
                        );
                        let refusal = SdkRefusal {
                            detail: SdkValue::map([
                                ("state", SdkValue::text("postgresql-reads-full")),
                                ("reads", SdkValue::Int(POSTGRES.slots as i64)),
                            ]),
                            ..refusal
                        };
                        let answered =
                            super::connection::refuse_host_request(self, &open, request, refusal);
                        stream = Some(open);
                        Ok(Served::Answered(answered))
                    }
                    _ => Ok(Served::HandedOver),
                };
            }
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
            self.queued[POSTGRES.index].notify_all();
        }
        self.notify();
    }

    /// Hands PostgreSQL read `request` on `stream` to its lane, or hands it
    /// back at once: while draining, or when the lane holds its `slots`
    /// reads (`Full`); the ordinary thread never waits here.
    fn hand_over(&self, stream: UnixStream, request: SdkValue) -> HandOver {
        let mut activity = self.lock();
        if activity.draining {
            return HandOver::Back(stream, request);
        }
        purge_gone(&mut activity);
        let queue = &mut activity.lanes[POSTGRES.index];
        if queue.outstanding >= POSTGRES.slots {
            return HandOver::Full(stream, request);
        }
        queue.outstanding += 1;
        queue.pending.push_back(Pending {
            stream,
            request: Some(request),
        });
        self.queued[POSTGRES.index].notify_all();
        HandOver::Queued
    }

    /// The oldest queued PostgreSQL read whose Store has a free session
    /// slot, with that slot; draining, the oldest read, to be refused.
    fn pop_postgres(
        &self,
    ) -> Option<(
        Pending,
        Option<layerfs_store::postgresql::pool::Reservation>,
    )> {
        let mut activity = self.lock();
        loop {
            purge_gone(&mut activity);
            let draining = activity.draining;
            let pending = &mut activity.lanes[POSTGRES.index].pending;
            let mut chosen = None;
            for (at, job) in pending.iter().enumerate() {
                // a read refused by its request alone takes no slot
                let store = job
                    .request
                    .as_ref()
                    .and_then(|request| self.runnable(request));
                match store {
                    Some(store) if !draining => {
                        if let Some(slot) =
                            layerfs_store::postgresql::pool::reserve(&self.home, store)
                        {
                            chosen = Some((at, Some(slot)));
                            break;
                        }
                    }
                    // refused before any session is taken
                    _ => {
                        chosen = Some((at, None));
                        break;
                    }
                }
            }
            if let Some((at, slot)) = chosen {
                return pending.remove(at).map(|job| (job, slot));
            }
            if draining {
                return None;
            }
            // every slot given back ends a read, whose `finish` wakes us
            activity = (self.queued[POSTGRES.index].wait(activity))
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    /// The Store of PostgreSQL read `request` when nothing in the request
    /// alone refuses it: this worker's generation, a method the route
    /// serves and a Store name. The record is checked when it runs.
    fn runnable<'a>(&self, request: &'a SdkValue) -> Option<&'a str> {
        if request.get_text("generation") != Some(self.hello.generation.as_str()) {
            return None;
        }
        let args = request.get("args")?;
        let method = args.get_text("method")?;
        layerfs_store::host_postgresql::serves(method).then_some(())?;
        let store = args.get_text("store")?;
        layerfs_home::names::StoreName::parse(store).ok()?;
        Some(store)
    }

    fn work_postgres(&self) {
        while let Some((Pending { stream, request }, slot)) = self.pop_postgres() {
            let mut admitted = false;
            if let Some(slot) = slot {
                layerfs_store::postgresql::pool::hold(slot);
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::connection::serve_host_connection(
                    self,
                    &stream,
                    false,
                    request,
                    &mut admitted,
                )
            }));
            layerfs_store::postgresql::pool::release_held();
            if result.is_err() {
                let _ = layerfs_home::trace::steps_take();
                log("a host PostgreSQL read panicked; its answer was lost");
            }
            drop(stream);
            self.finish(POSTGRES, admitted, &result);
        }
    }

    fn accept(&self, listener: &UnixListener, lane: Lane) -> io::Result<()> {
        // Only this thread accepts. A worker can free a slot during poll,
        // but it cannot consume one, so readiness never overbooks the lane.
        let mut activity = self.lock();
        if activity.draining {
            return Ok(());
        }
        let queue = &mut activity.lanes[lane.index];
        if queue.outstanding == lane.slots {
            return Ok(());
        }
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
        ordinary: &UnixListener,
        control: &UnixListener,
        mut wake_read: UnixStream,
        unpublish: &dyn Fn(),
    ) -> io::Result<()> {
        let mut limited = super::Limited {
            every: Duration::from_secs(10),
            kinds: Vec::new(),
        };
        // §8.3 idle exit: first remove the socket names, so no new caller can
        // join a listen queue; keep serving whatever is already queued; drain
        // only once quiet with both queues empty. A caller after the removal
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
                            && !queued(control)? =>
                    {
                        self.drain(&mut activity);
                        return Ok(());
                    }
                    _ => {}
                }
            }
            let interests = |lane: Lane| {
                if activity.lanes[lane.index].outstanding < lane.slots {
                    PollFlags::POLLIN
                } else {
                    PollFlags::empty()
                }
            };
            let mut ready = [
                PollFd::new(wake_read.as_fd(), PollFlags::POLLIN),
                PollFd::new(control.as_fd(), interests(CONTROL)),
                PollFd::new(ordinary.as_fd(), interests(ORDINARY)),
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

    pub(super) fn dispatch(
        &self,
        control: bool,
        op: &str,
        args: &SdkValue,
        admitted: &mut bool,
    ) -> Outcome {
        // Admission and drain share one lock. Store work later runs after
        // admission, outside this lock, on the ordinary pool.
        let stores =
            (control && op == "ping").then(|| layerfs_store::open::host_store_count(&self.home));
        let mut activity = self.lock();
        if activity.draining {
            let mut error = refused("Unavailable", "the host worker is draining");
            error.detail = SdkValue::map([("state", SdkValue::text("draining"))]);
            return Err(error);
        }
        enum Operation {
            Ping,
            Drain,
            CloseStore,
            StoreCall,
            PostgresRead,
            PostgresRetire,
        }
        let operation = match (control, op) {
            (true, "ping") => Operation::Ping,
            (true, "drain") => Operation::Drain,
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
        activity.admitted += 1;
        *admitted = true;
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
                    ("stores", SdkValue::Int(stores.unwrap_or_default() as i64)),
                    ("postgresql", postgresql(&activity)),
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
            Operation::Drain => {
                self.drain(&mut activity);
                drop(activity);
                self.close_drained_stores();
                Ok(SdkValue::map([("state", SdkValue::text("draining"))]))
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

/// Where a PostgreSQL read went.
enum HandOver {
    Queued,
    /// Draining: answered by the ordinary thread.
    Back(UnixStream, SdkValue),
    /// The lane holds its `slots` reads: answered `Unavailable` at once,
    /// unrun.
    Full(UnixStream, SdkValue),
}

/// Drops queued PostgreSQL reads whose caller has closed its end: none of
/// them has started, and none is run.
fn purge_gone(activity: &mut Activity) {
    let queue = &mut activity.lanes[POSTGRES.index];
    let before = queue.pending.len();
    let mut gone = Vec::new();
    queue.pending.retain_mut(|job| {
        let open = !peer_gone(&job.stream);
        if !open {
            gone.push(job.request.take());
        }
        open
    });
    queue.outstanding -= before - queue.pending.len();
    for request in gone.into_iter().flatten() {
        layerfs_store::host_postgresql::wipe_request(request);
    }
}

/// Whether the caller of `stream` has closed or reset its end: a hang-up,
/// an error, or end of file seen by a peek that consumes nothing. Bytes the
/// caller sends are not a close: a streamed P-B write (#819, Step 7) may
/// send more after its request, and stays queued. That step must also
/// handle macOS, where `poll` may report `POLLHUP` once the caller shuts
/// down only its write side, which is not a close of the read side.
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

/// What one connection came to on its thread.
pub(super) enum Served {
    /// Answered (`true`) or ended without an answer.
    Answered(bool),
    /// A PostgreSQL read whose request was read, for its own lane.
    Postgres(SdkValue),
    /// Handed to the PostgreSQL lane, which answers and accounts it.
    HandedOver,
}

/// The ping's `postgresql` field (§11): held read sessions idle and lent,
/// calls waiting for one, and reads handed over and not yet answered.
fn postgresql(activity: &Activity) -> SdkValue {
    let counts = layerfs_store::postgresql::pool::counts();
    SdkValue::map([
        ("stores", SdkValue::Int(counts.stores as i64)),
        ("idle", SdkValue::Int(counts.idle as i64)),
        ("lent", SdkValue::Int(counts.lent as i64)),
        (
            "reads",
            SdkValue::Int(activity.lanes[POSTGRES.index].outstanding as i64),
        ),
        (
            "queued",
            SdkValue::Int(activity.lanes[POSTGRES.index].pending.len() as i64),
        ),
    ])
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
    ordinary: UnixListener,
    control: UnixListener,
    hello: HostHello,
    home: PathBuf,
    home_id: String,
    unpublish: &dyn Fn(),
) -> io::Result<super::Served> {
    ordinary.set_nonblocking(true)?;
    control.set_nonblocking(true)?;
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
        wake,
    };
    let served = std::thread::scope(|scope| {
        let result = (|| {
            let server = &server;
            for lane in [CONTROL, ORDINARY, POSTGRES] {
                for _ in 0..lane.threads {
                    std::thread::Builder::new()
                        .name(lane.name.to_owned())
                        .spawn_scoped(scope, move || match lane.index == POSTGRES.index {
                            true => server.work_postgres(),
                            false => server.work(lane),
                        })?;
                }
            }
            server.listen(&ordinary, &control, wake_read, unpublish)
        })();
        server.drain(&mut server.lock());
        drop(ordinary);
        drop(control);
        // Scope exit joins accepted connections before releasing the owner.
        result
    });
    server.close_drained_stores();
    served?;
    let requests = server.lock().answered;
    Ok(super::Served { requests })
}

/// Runs the host worker with the real home and its lifetime lock.
/// This entry runs in its own process, above the Store and machine crates.
pub fn serve_host(home: &Path) -> Result<super::Served, SdkRefusal> {
    // before any request, which may carry a PostgreSQL password, is read
    layerfs_store::host_postgresql::limit_core_dumps();
    let paths = HostPaths::new(home)?;
    let guard = paths.try_hold()?.ok_or_else(|| {
        refused(
            "Unavailable",
            "another host worker owns the endpoint for this home",
        )
    })?;
    let run = (|| {
        let hello = HostHello::own(&paths)?;
        let (ordinary, control) = {
            let _home = layerfs_home::home::lock::take(home)?;
            layerfs_machine::record::MachineRecord::admit_host(
                home,
                layerfs_home::names::HOST_MACHINE,
            )?;
            (paths.bind(false, &guard)?, paths.bind(true, &guard)?)
        };
        run(
            ordinary,
            control,
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
