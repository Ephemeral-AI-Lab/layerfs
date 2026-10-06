//! Readiness, short owner jobs and fair typed admission; no whole-operation lock.
use crate::{
    commands::{Command, Response, ServiceClass},
    credits::Credit,
    queue::{Envelope, Job, Lane, OwnerWork, Shared},
};
use layerfs_overlay::{DatabaseProfile, Overlay, OverlayError, ProfileConfig, Route};
use std::{
    fmt, io,
    path::Path,
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

#[derive(Clone, Copy, Debug)]
pub struct OwnerConfig {
    pub bytes: usize,
    pub lifecycle_reserve: usize,
    pub namespaces: usize,
    pub jobs_per_namespace: usize,
    pub lifecycle_jobs_per_namespace: usize,
}
impl Default for OwnerConfig {
    fn default() -> Self {
        Self {
            bytes: 8 * 1024 * 1024,
            lifecycle_reserve: 64 * 1024,
            namespaces: 16,
            jobs_per_namespace: 16,
            lifecycle_jobs_per_namespace: 2,
        }
    }
}
#[derive(Debug)]
pub enum OwnerError {
    Io(io::Error),
    Overlay(OverlayError),
    Stopped,
    AdmissionFull,
    InvalidAdmission,
    IdentityExhausted,
    Disconnected,
    WorkerPanicked,
    Unattempted {
        cause: Box<OwnerError>,
        command: Box<Command>,
    },
    Install {
        cause: Box<layerfs_workspace::WorkspaceError>,
        input: Box<layerfs_workspace::PreparedBase>,
    },
    /// Exact failure of an attempted namespace job; never replayed.
    Workspace(Box<layerfs_workspace::WorkspaceError>),
}
impl fmt::Display for OwnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OwnerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Overlay(error) => Some(error),
            Self::Unattempted { cause, .. } => Some(cause.as_ref()),
            Self::Install { cause, .. } => Some(cause.as_ref()),
            Self::Workspace(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}

/// One initialized daemon service. It owns the connection thread's entire lifetime.
pub struct Owner {
    client: OwnerClient,
    worker: Option<JoinHandle<()>>,
    profile: DatabaseProfile,
    startup: Arc<layerfs_overlay::CreationWork>,
}
#[derive(Clone)]
pub struct OwnerClient {
    shared: Arc<Shared>,
}
/// A pending original operation. Waiting never occupies a native dispatch worker
/// unless its caller chooses to wait synchronously there.
pub struct Pending {
    receiver: Receiver<Envelope>,
    credit: Arc<Credit>,
}
/// Result and its retained aggregate credit. Data is borrowed until this drops.
pub struct Completion {
    envelope: Envelope,
}
impl Completion {
    /// Complete exclusive SQL/allocation work, including readiness turns which
    /// parked this original job. Queue/service spans are diagnostic wall.
    pub fn work(&self) -> &crate::JobWork {
        &self.envelope.work
    }
    pub fn result(&self) -> &Result<Response, OwnerError> {
        &self.envelope.result
    }
}
impl Pending {
    pub fn wait(self) -> Result<Completion, OwnerError> {
        let _credit = &self.credit;
        self.receiver
            .recv()
            .map(|envelope| Completion { envelope })
            .map_err(|_| OwnerError::Disconnected)
    }
    pub fn try_complete(&self) -> Result<Option<Completion>, OwnerError> {
        match self.receiver.try_recv() {
            Ok(envelope) => Ok(Some(Completion { envelope })),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(OwnerError::Disconnected),
        }
    }
}
impl Owner {
    /// Initializes exactly one overlay before reporting readiness. No retries.
    pub fn start(
        path: &Path,
        profile: ProfileConfig,
        config: OwnerConfig,
    ) -> Result<Self, OwnerError> {
        Self::start_observed(path, profile, config).result
    }

    /// Retains original initialization work even when startup fails. This does
    /// not retry creation or remove the failed database artifact.
    pub fn start_observed(
        path: &Path,
        profile: ProfileConfig,
        config: OwnerConfig,
    ) -> crate::OwnerStart {
        let started = Instant::now();
        let mut startup = Arc::new(layerfs_overlay::CreationWork::default());
        let result = (|| {
            if config.bytes <= config.lifecycle_reserve
                || config.namespaces == 0
                || config.jobs_per_namespace == 0
                || config.lifecycle_jobs_per_namespace == 0
            {
                return Err(OwnerError::InvalidAdmission);
            }
            let reserved = config
                .namespaces
                .checked_mul(config.lifecycle_jobs_per_namespace)
                .and_then(|slots| {
                    slots.checked_mul(
                        std::mem::size_of::<Command>()
                            + std::mem::size_of::<crate::JobWork>()
                            + 768,
                    )
                })
                .ok_or(OwnerError::InvalidAdmission)?;
            if config.lifecycle_reserve < reserved {
                return Err(OwnerError::InvalidAdmission);
            }
            let shared = Arc::new(Shared::new(config));
            let owner_shared = shared.clone();
            let path = path.to_owned();
            let (ready, receiver) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name("layerfs-overlay-owner".into())
                .spawn(move || {
                    let created = Overlay::create_observed(&path, profile);
                    let startup = Arc::new(created.work);
                    let db = match created.result {
                        Ok(db) => db,
                        Err(error) => {
                            let _ = ready.send((Err(OwnerError::Overlay(error)), startup));
                            return;
                        }
                    };
                    if ready.send((Ok(db.profile().clone()), startup)).is_err() {
                        return;
                    }
                    run(&owner_shared, &db);
                })
                .map_err(OwnerError::Io)?;
            let profile = match receiver.recv() {
                Ok((Ok(profile), observed)) => {
                    startup = observed;
                    profile
                }
                Ok((Err(error), observed)) => {
                    startup = observed;
                    worker.join().map_err(|_| OwnerError::WorkerPanicked)?;
                    return Err(error);
                }
                Err(_) => {
                    worker.join().map_err(|_| OwnerError::WorkerPanicked)?;
                    return Err(OwnerError::Disconnected);
                }
            };
            Ok(Self {
                client: OwnerClient { shared },
                worker: Some(worker),
                profile,
                startup: startup.clone(),
            })
        })();
        crate::OwnerStart {
            result,
            startup,
            elapsed_ns: started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
        }
    }

    /// Finite creation receipt, separate from all foreground/maintenance jobs.
    pub fn startup_work(&self) -> &layerfs_overlay::CreationWork {
        &self.startup
    }
    pub fn client(&self) -> OwnerClient {
        self.client.clone()
    }
    pub fn profile(&self) -> &DatabaseProfile {
        &self.profile
    }
    /// Stops this daemon service, cancelling only unattempted queued jobs. Whole
    /// daemon teardown is distinct from a single Workspace terminal unmount.
    pub fn stop(mut self) -> Result<(), OwnerError> {
        self.client.shared.stop();
        self.worker
            .take()
            .ok_or(OwnerError::Stopped)?
            .join()
            .map_err(|_| OwnerError::WorkerPanicked)
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.client.shared.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl OwnerClient {
    /// Admit a bounded job or return the original owned command without effect.
    /// Native callers can defer outside their workers until credits are available.
    pub fn try_submit(
        &self,
        route: Option<Route>,
        command: Command,
    ) -> Result<Pending, (OwnerError, Command)> {
        if route.is_none() != matches!(command, Command::Open { .. }) {
            return Err((OwnerError::InvalidAdmission, command));
        }
        let Some(bytes) = command.charge() else {
            return Err((OwnerError::InvalidAdmission, command));
        };
        let class = command.class();
        let namespace = route.map_or(0, Route::namespace);
        let mut state = match self.shared.state.lock() {
            Ok(s) => s,
            Err(_) => return Err((OwnerError::Stopped, command)),
        };
        if state.stopping {
            return Err((OwnerError::Stopped, command));
        }
        let config = self.shared.config;
        let limit = if class == ServiceClass::Lifecycle {
            config.bytes
        } else {
            config.bytes - config.lifecycle_reserve
        };
        if bytes > limit || state.work.credited_bytes > limit - bytes {
            return Err((OwnerError::AdmissionFull, command));
        }
        let new = !state.lanes.contains_key(&namespace);
        if new && state.lanes.len() == config.namespaces {
            return Err((OwnerError::AdmissionFull, command));
        }
        if let Some(lane) = state.lanes.get(&namespace) {
            if (class == ServiceClass::Lifecycle
                && lane.lifecycle == config.lifecycle_jobs_per_namespace)
                || (class != ServiceClass::Lifecycle && lane.ordinary == config.jobs_per_namespace)
            {
                return Err((OwnerError::AdmissionFull, command));
            }
        }
        let id = state.next;
        let Some(next) = id.checked_add(1) else {
            return Err((OwnerError::IdentityExhausted, command));
        };
        state.next = next;
        state.event = state.event.wrapping_add(1);
        let credit = Arc::new(Credit {
            shared: Arc::downgrade(&self.shared),
            namespace,
            bytes,
            class,
        });
        let (reply, receiver) = mpsc::sync_channel(1);
        let lane = state.lanes.entry(namespace).or_insert_with(Lane::new);
        if class == ServiceClass::Lifecycle {
            lane.lifecycle += 1;
        } else {
            lane.ordinary += 1;
        }
        lane.queues[class as usize].push_back(Job {
            id,
            route,
            command,
            credit: credit.clone(),
            reply,
            admitted: Instant::now(),
            blocked: false,
            wait_consolidation: false,
            work: crate::JobWork::default(),
        });
        if new {
            state.rotation.push_back(namespace);
        }
        state.work.credited_bytes += bytes;
        state.work.outstanding += 1;
        state.work.peak_credited_bytes = state
            .work
            .peak_credited_bytes
            .max(state.work.credited_bytes);
        state.work.admitted = state.work.admitted.saturating_add(1);
        drop(state);
        self.shared.wake.notify_one();
        Ok(Pending { receiver, credit })
    }
    pub fn diagnostics(&self) -> Result<OwnerWork, OwnerError> {
        self.shared
            .state
            .lock()
            .map(|s| s.work)
            .map_err(|_| OwnerError::Stopped)
    }
    /// First automatic-maintenance error, retained without an automatic retry.
    pub fn maintenance_failure(&self) -> Result<Option<Arc<OverlayError>>, OwnerError> {
        self.shared
            .state
            .lock()
            .map(|s| s.maintenance_error.clone())
            .map_err(|_| OwnerError::Stopped)
    }
}
fn ns(route: Option<Route>) -> i64 {
    route.map_or(0, Route::namespace)
}
fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
struct SqlObservation<'a> {
    db: &'a Overlay,
    shared: &'a Shared,
    before: layerfs_overlay::DatabaseWork,
    maintenance: bool,
    allocation_before: layerfs_overlay::AllocationWork,
    payload_before: layerfs_overlay::PayloadWork,
}
impl Drop for SqlObservation<'_> {
    fn drop(&mut self) {
        self.shared.sql_progress(
            self.db.diagnostics().since(&self.before),
            self.db.allocation_work().since(self.allocation_before),
            self.db.payload_work().since(self.payload_before),
            self.maintenance,
        );
    }
}
fn run(shared: &Shared, db: &Overlay) {
    let mut served = 0_u8;
    let mut cursor = 0_u64;
    let mut maintenance_failed = false;
    let mut live_cursor = layerfs_overlay::MaintenanceCursor::default();
    let mut live_turn = true;
    while let Some((job, event)) = shared.poll() {
        let mut maintained = false;
        if !maintenance_failed && (served >= 8 || job.is_none()) {
            let _sql = SqlObservation {
                db,
                shared,
                before: db.diagnostics(),
                maintenance: true,
                allocation_before: db.allocation_work(),
                payload_before: db.payload_work(),
            };
            let start = Instant::now();
            let maintenance = maintenance_turn(db, &mut live_cursor, &mut cursor, &mut live_turn);
            served = 0;
            match maintenance {
                Ok(Some(step)) => {
                    shared.maintenance(step, elapsed(start));
                    served = 0;
                    maintained = true;
                }
                Ok(None) => {}
                Err(error) => {
                    maintenance_failed = true;
                    shared.maintenance_failed(error);
                }
            }
        }
        let Some(mut job) = job else {
            if !maintained {
                shared.wait(event);
            }
            continue;
        };
        served = served.saturating_add(1);
        if shared.stopped() {
            let _ = job.reply.send(Envelope {
                result: Err(OwnerError::Unattempted {
                    cause: Box::new(OwnerError::Stopped),
                    command: Box::new(job.command),
                }),
                work: job.work,
                _credit: job.credit,
            });
            continue;
        }
        let _sql = SqlObservation {
            db,
            shared,
            before: db.diagnostics(),
            maintenance: false,
            allocation_before: db.allocation_work(),
            payload_before: db.payload_work(),
        };
        let before = db.diagnostics();
        let allocation_before = db.allocation_work();
        let payload_before = db.payload_work();
        if matches!(job.command, Command::Capture) {
            if let Some(route) = job.route {
                match db.capture_ready(route) {
                    Ok(false) => {
                        match db.state(route) {
                            Ok(state) => job.wait_consolidation = state.consolidating.is_some(),
                            Err(error) => {
                                record_job_work(
                                    &mut job,
                                    db,
                                    before,
                                    allocation_before,
                                    payload_before,
                                );
                                let _ = job.reply.send(Envelope {
                                    result: Err(OwnerError::Unattempted {
                                        cause: Box::new(OwnerError::Overlay(error)),
                                        command: Box::new(job.command),
                                    }),
                                    work: job.work,
                                    _credit: job.credit,
                                });
                                continue;
                            }
                        }
                        record_job_work(&mut job, db, before, allocation_before, payload_before);
                        shared.park(job);
                        continue;
                    }
                    Err(error) => {
                        let class = job.command.class();
                        shared.progress(ns(job.route), class, elapsed(job.admitted), 0);
                        record_job_work(&mut job, db, before, allocation_before, payload_before);
                        let _ = job.reply.send(Envelope {
                            result: Err(OwnerError::Unattempted {
                                cause: Box::new(OwnerError::Overlay(error)),
                                command: Box::new(job.command),
                            }),
                            work: job.work,
                            _credit: job.credit,
                        });
                        continue;
                    }
                    Ok(true) => {}
                }
            }
        }
        if let Some(capture) = job.command.install_capture() {
            let ready = if job.route == Some(capture.route()) {
                db.install_ready(capture)
            } else {
                Err(OverlayError::Stale)
            };
            match ready {
                Ok(false) => {
                    record_job_work(&mut job, db, before, allocation_before, payload_before);
                    shared.park(job);
                    continue;
                }
                Err(error) => {
                    let class = job.command.class();
                    shared.progress(ns(job.route), class, elapsed(job.admitted), 0);
                    record_job_work(&mut job, db, before, allocation_before, payload_before);
                    let _ = job.reply.send(Envelope {
                        result: Err(OwnerError::Unattempted {
                            cause: Box::new(OwnerError::Overlay(error)),
                            command: Box::new(job.command),
                        }),
                        work: job.work,
                        _credit: job.credit,
                    });
                    continue;
                }
                Ok(true) => {}
            }
        }
        let class = job.command.class();
        let namespace = ns(job.route);
        let wait = elapsed(job.admitted);
        let start = Instant::now();
        let result = job.command.perform(db, job.route);
        job.work
            .sql
            .accumulate(db.diagnostics().since(&before).total());
        job.work
            .allocation
            .accumulate(db.allocation_work().since(allocation_before));
        job.work
            .payload
            .accumulate(db.payload_work().since(payload_before));
        job.work.queue_wait_ns = wait;
        job.work.service_ns = elapsed(start);
        drop(_sql);
        shared.progress(namespace, class, wait, job.work.service_ns);
        let _ = job.reply.send(Envelope {
            result,
            work: job.work,
            _credit: job.credit,
        });
    }
}

fn maintenance_turn(
    db: &Overlay,
    live: &mut layerfs_overlay::MaintenanceCursor,
    closed: &mut u64,
    live_turn: &mut bool,
) -> Result<Option<layerfs_overlay::ReclaimStep>, OverlayError> {
    let first = *live_turn;
    *live_turn = !*live_turn;
    for use_live in [first, !first] {
        if use_live {
            if let Some(step) = db.maintain(*live)? {
                *live = step.cursor;
                return Ok(Some(step.work));
            }
        } else if let Some(step) = db.reclaim_closed(*closed)? {
            *closed = step.namespace;
            return Ok(Some(step));
        }
    }
    Ok(None)
}

impl fmt::Debug for Completion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.envelope.result.fmt(f)
    }
}
impl fmt::Display for Completion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "owner completion: {:?}", self.envelope.result)
    }
}
impl std::error::Error for Completion {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.envelope
            .result
            .as_ref()
            .err()
            .map(|error| error as &dyn std::error::Error)
    }
}

fn record_job_work(
    job: &mut Job,
    db: &Overlay,
    before: layerfs_overlay::DatabaseWork,
    allocation: layerfs_overlay::AllocationWork,
    payload: layerfs_overlay::PayloadWork,
) {
    job.work
        .sql
        .accumulate(db.diagnostics().since(&before).total());
    job.work
        .allocation
        .accumulate(db.allocation_work().since(allocation));
    job.work
        .payload
        .accumulate(db.payload_work().since(payload));
    job.work.queue_wait_ns = elapsed(job.admitted);
}
