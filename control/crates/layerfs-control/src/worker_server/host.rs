//! Host worker on the real home, with separate admission for management.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_machine::worker::{HostHello, HostPaths};
use nix::poll::{poll, PollFd, PollFlags};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(60);
const IDLE_CHECK: Duration = Duration::from_secs(5);
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

#[derive(Clone, Copy)]
struct Lane {
    index: usize,
    slots: usize,
    threads: usize,
    name: &'static str,
}

struct Pending {
    stream: UnixStream,
    accepted: Instant,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<Pending>,
    /// Accepted sockets in this lane, including the queue and all exchanges.
    outstanding: usize,
}

struct Activity {
    lanes: [Queue; 2],
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

    fn idle(&self) -> bool {
        self.admitted == 0 && self.last_call.elapsed() >= IDLE
    }
}

pub(super) struct HostServer {
    pub(super) hello: HostHello,
    home: PathBuf,
    home_id: String,
    identity: Vec<(&'static str, SdkValue)>,
    activity: Mutex<Activity>,
    queued: [Condvar; 2],
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
        while let Some(connection) = self.pop(lane) {
            let mut admitted = false;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::connection::serve_host_connection(
                    self,
                    &connection.stream,
                    connection.accepted,
                    lane.index == CONTROL.index,
                    &mut admitted,
                )
            }));
            if result.is_err() {
                let _ = layerfs_home::trace::steps_take();
                log("a host connection panicked; its answer was lost");
            }
            drop(connection);
            let mut activity = self.lock();
            activity.lanes[lane.index].outstanding -= 1;
            if admitted {
                activity.admitted -= 1;
                activity.last_call = Instant::now();
            }
            if matches!(result, Ok(true)) {
                activity.answered += 1;
            }
            drop(activity);
            self.notify();
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
                    accepted: Instant::now(),
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
    ) -> io::Result<()> {
        let mut limited = super::Limited {
            every: Duration::from_secs(10),
            kinds: Vec::new(),
        };
        loop {
            let mut activity = self.lock();
            if activity.draining {
                return Ok(());
            }
            if activity.idle() {
                self.drain(&mut activity);
                return Ok(());
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
            match poll(&mut ready, IDLE_CHECK.as_millis() as u16) {
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
        }
        let operation = match (control, op) {
            (true, "ping") => Operation::Ping,
            (true, "drain") => Operation::Drain,
            (true, layerfs_store::host_maintenance::CLOSE) => Operation::CloseStore,
            (false, layerfs_store::host_read::OP) => Operation::StoreCall,
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

fn log(message: &str) {
    let _ = writeln!(io::stderr(), "layerfs host worker: {message}");
}

fn run(
    ordinary: UnixListener,
    control: UnixListener,
    hello: HostHello,
    home: PathBuf,
    home_id: String,
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
            for lane in [CONTROL, ORDINARY] {
                for _ in 0..lane.threads {
                    std::thread::Builder::new()
                        .name(lane.name.to_owned())
                        .spawn_scoped(scope, move || server.work(lane))?;
                }
            }
            server.listen(&ordinary, &control, wake_read)
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

/// Runs the native host worker with the real home and its lifetime lock.
/// This entry runs in its own process, above the Store and machine crates.
pub fn serve_host(home: &Path) -> Result<super::Served, SdkRefusal> {
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
        )
        .map_err(failed)
    })();
    let cleanup = paths.remove_endpoints(&guard);
    if let Err(error) = &cleanup {
        log(&format!("socket cleanup failed: {}", error.message));
    }
    run.and_then(|served| cleanup.map(|()| served))
}
