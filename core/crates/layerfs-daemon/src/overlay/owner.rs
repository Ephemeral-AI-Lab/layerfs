//! Readiness, short owner jobs and fair typed admission; no whole-operation lock.
use crate::{
    commands::{Command, ServiceClass},
    credits::Credit,
    queue::{Job, OwnerWork, Shared},
    service::completion::{self, Outcome, Pending},
};
use layerfs_overlay::{DatabaseProfile, Overlay, OverlayError, ProfileConfig, Route};
use std::{
    fmt, io,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{atomic::AtomicUsize, mpsc, Arc, Mutex},
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
    /// Original join payload. The mutex preserves Sync error custody even
    /// when the caller's original panic value is Send but not Sync.
    WorkerPanicked(Mutex<Box<dyn std::any::Any + Send>>),
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

/// Worker loss closes admission and disposes only queued, unattempted jobs.
/// The joining owner retains the original panic, of the owner thread or of a
/// submitting thread's turn; published outcomes stay intact. The connection
/// closes with its owner thread, after any turn a submitting thread holds.
struct ExitFence<'a>(&'a Shared);
impl Drop for ExitFence<'_> {
    fn drop(&mut self) {
        self.0.stop();
        let closed = self.0.connection().take();
        drop(closed);
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
    pub(super) shared: Arc<Shared>,
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
        let mut creation_reported = false;
        let result = (|| {
            if config.bytes <= config.lifecycle_reserve
                || config.namespaces == 0
                || config.jobs_per_namespace == 0
                || config.lifecycle_jobs_per_namespace == 0
            {
                return Err(OwnerError::InvalidAdmission);
            }
            // Every configured lifecycle slot must fit the reserve at its real
            // charge, and the fixed scheduler state must leave ordinary bytes.
            let reserved = config
                .namespaces
                .checked_mul(config.lifecycle_jobs_per_namespace)
                .zip(Command::lifecycle_charge())
                .and_then(|(slots, charge)| slots.checked_mul(charge))
                .ok_or(OwnerError::InvalidAdmission)?;
            if config.lifecycle_reserve < reserved
                || Shared::planned_bytes(config)
                    .is_none_or(|fixed| fixed >= config.bytes - config.lifecycle_reserve)
            {
                return Err(OwnerError::InvalidAdmission);
            }
            let shared = Arc::new(Shared::new(config));
            if shared.scheduler_bytes() >= config.bytes - config.lifecycle_reserve {
                return Err(OwnerError::InvalidAdmission);
            }
            let owner_shared = shared.clone();
            let path = path.to_owned();
            let (ready, receiver) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name("layerfs-overlay-owner".into())
                .spawn(move || {
                    let _exit = ExitFence(&owner_shared);
                    let created = Overlay::create_observed(&path, profile);
                    let startup = Arc::new(created.work);
                    let db = match created.result {
                        Ok(db) => db,
                        Err(error) => {
                            let _ = ready.send((Err(OwnerError::Overlay(error)), startup));
                            return;
                        }
                    };
                    // The connection is in place before readiness is
                    // reported: the first submitted job may take its turn on
                    // the submitting thread.
                    let profile = db.profile().clone();
                    *owner_shared.connection() = Some(Box::new(db));
                    if ready.send((Ok(profile), startup)).is_ok() {
                        run(&owner_shared);
                    }
                })
                .map_err(OwnerError::Io)?;
            let profile = match receiver.recv() {
                Ok((Ok(profile), observed)) => {
                    startup = observed;
                    creation_reported = true;
                    profile
                }
                Ok((Err(error), observed)) => {
                    startup = observed;
                    creation_reported = true;
                    worker
                        .join()
                        .map_err(|payload| OwnerError::WorkerPanicked(Mutex::new(payload)))?;
                    return Err(error);
                }
                Err(_) => {
                    worker
                        .join()
                        .map_err(|payload| OwnerError::WorkerPanicked(Mutex::new(payload)))?;
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
            creation_reported,
            elapsed_ns: started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
        }
    }

    /// Finite creation receipt, separate from all foreground/maintenance jobs.
    pub fn startup_work(&self) -> &layerfs_overlay::CreationWork {
        &self.startup
    }
    /// Fixed daemon admission configuration shared with control routing.
    pub fn configuration(&self) -> OwnerConfig {
        self.client.shared.config
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
            .map_err(|payload| OwnerError::WorkerPanicked(Mutex::new(payload)))?;
        match self.client.shared.panicked() {
            Some(payload) => Err(OwnerError::WorkerPanicked(Mutex::new(payload))),
            None => Ok(()),
        }
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
        if (route.is_none()
            && !matches!(
                command,
                Command::Open { .. }
                    | Command::ObserveCleanup { .. }
                    | Command::Resources { global: true }
            ))
            || (route.is_some()
                && matches!(
                    command,
                    Command::Open { .. } | Command::ObserveCleanup { .. }
                ))
        {
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
        // Fixed scheduler state is never available to jobs; ordinary classes
        // additionally leave the lifecycle reserve.
        let limit = self.shared.job_capacity(class, &state.work);
        if bytes > limit || state.work.credited_bytes > limit - bytes {
            return Err((OwnerError::AdmissionFull, command));
        }
        // Source acquisitions and the other four ordinary classes each have
        // `jobs_per_namespace` slots of a lane; Lifecycle has its own bound.
        let bound = if class == ServiceClass::Lifecycle {
            config.lifecycle_jobs_per_namespace
        } else {
            config.jobs_per_namespace
        };
        let active = |lane: &crate::queue::Lane| lane.active && lane.namespace == namespace;
        let slot = match state.lanes.iter().position(active) {
            Some(slot) => {
                if *state.lanes[slot].slots(class) == bound {
                    return Err((OwnerError::AdmissionFull, command));
                }
                slot
            }
            None => match state.lanes.iter().position(|lane| !lane.active) {
                Some(slot) => slot,
                None => return Err((OwnerError::AdmissionFull, command)),
            },
        };
        let id = state.next;
        let Some(next) = id.checked_add(1) else {
            return Err((OwnerError::IdentityExhausted, command));
        };
        state.next = next;
        state.event = state.event.wrapping_add(1);
        let (publisher, pending) = completion::admit(Credit {
            shared: Arc::downgrade(&self.shared),
            namespace,
            bytes: AtomicUsize::new(bytes),
            class,
        });
        let lane = &mut state.lanes[slot];
        let new = !lane.active;
        if new {
            lane.active = true;
            lane.namespace = namespace;
            lane.next = 0;
        }
        *lane.slots(class) += 1;
        lane.queues[class as usize].push_back(Box::new(Job {
            id,
            route,
            command,
            publisher,
            admitted: Instant::now(),
            blocked: false,
            wait_consolidation: false,
            work: crate::JobWork::default(),
        }));
        if new {
            state.rotation.push_back(namespace);
        }
        state.queued();
        state.work.credited_bytes += bytes;
        state.work.outstanding += 1;
        state.work.peak_credited_bytes = state
            .work
            .peak_credited_bytes
            .max(state.work.credited_bytes);
        state.work.admitted = state.work.admitted.saturating_add(1);
        // With nothing else queued and the connection free, this thread
        // takes the turn itself: the same selection, readiness and service
        // as the owner thread's, without a hand-off to it and back.
        // Pending maintenance is never held back by more than eight turns.
        let turn = if !state.busy && state.work.queued == 1 && !state.maintenance_due() {
            state.take()
        } else {
            None
        };
        match turn {
            Some(job) => {
                state.busy = true;
                drop(state);
                serve_submitted(&self.shared, job);
            }
            None => {
                drop(state);
                self.shared.wake.notify_all();
            }
        }
        Ok(pending)
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
/// Connection snapshots around one exclusive owner turn.
struct Turn<'a> {
    db: &'a Overlay,
    sql: layerfs_overlay::DatabaseWork,
    allocation: layerfs_overlay::AllocationWork,
    payload: layerfs_overlay::PayloadWork,
}
impl<'a> Turn<'a> {
    fn begin(db: &'a Overlay) -> Self {
        Self {
            db,
            sql: db.diagnostics(),
            allocation: db.allocation_work(),
            payload: db.payload_work(),
        }
    }
    /// Publishes this turn to an owner aggregate. A job's receipt takes the
    /// same deltas, so original-job sums equal the foreground aggregate, and
    /// the aggregate is current before that job's completion is visible.
    fn settle(self, shared: &Shared, job: Option<&mut crate::JobWork>) {
        let sql = self.db.diagnostics().since(&self.sql);
        let allocation = self.db.allocation_work().since(self.allocation);
        let payload = self.db.payload_work().since(self.payload);
        let maintenance = job.is_none();
        if let Some(work) = job {
            work.sql.accumulate(&sql);
            work.allocation.accumulate(allocation);
            work.payload.accumulate(payload);
        }
        shared.sql_progress(sql, allocation, payload, maintenance);
    }
}
/// Read-only readiness of a capture or known install before its one attempt.
fn ready(db: &Overlay, job: &mut Job) -> Result<bool, OverlayError> {
    if matches!(job.command, Command::Capture) {
        if let Some(route) = job.route {
            if !db.capture_ready(route)? {
                job.wait_consolidation = db.state(route)?.consolidating.is_some();
                return Ok(false);
            }
        }
    }
    if let Some(capture) = job.command.install_capture() {
        if job.route != Some(capture.route()) {
            return Err(OverlayError::Stale);
        }
        return db.install_ready(capture);
    }
    Ok(true)
}
/// A turn taken by the submitting thread. A panic in the job leaves the
/// connection in an unknown state: admission closes, queued jobs get their
/// commands back unattempted, the turn is never returned and the panic is
/// kept for `Owner::stop`. The submitter's own result reads disconnected.
fn serve_submitted(shared: &Shared, job: Box<Job>) {
    let served = catch_unwind(AssertUnwindSafe(|| {
        let connection = shared.connection();
        match connection.as_deref() {
            Some(db) => {
                serve(shared, db, job);
                db.maintenance_pending()
            }
            None => {
                job.refuse(OwnerError::Stopped);
                false
            }
        }
    }));
    match served {
        Ok(maintenance) => shared.idle(maintenance, false),
        Err(payload) => {
            shared.stop();
            shared.panic(payload);
        }
    }
}
/// One job on the connection, from its readiness check to its published
/// outcome. The caller holds the turn.
fn serve(shared: &Shared, db: &Overlay, mut job: Box<Job>) {
    if shared.stopped() {
        job.refuse(OwnerError::Stopped);
        return;
    }
    let turn = Turn::begin(db);
    match ready(db, &mut job) {
        Ok(true) => {}
        Ok(false) => {
            turn.settle(shared, Some(&mut job.work));
            job.work.queue_wait_ns = elapsed(job.admitted);
            shared.park(job);
            return;
        }
        Err(error) => {
            turn.settle(shared, Some(&mut job.work));
            job.work.queue_wait_ns = elapsed(job.admitted);
            shared.progress(
                ns(job.route),
                job.command.class(),
                job.work.queue_wait_ns,
                0,
            );
            job.refuse(OwnerError::Overlay(error));
            return;
        }
    }
    let class = job.command.class();
    let namespace = ns(job.route);
    let wait = elapsed(job.admitted);
    let start = Instant::now();
    // The queued job's storage is released before its outcome exists.
    let Job {
        route,
        command,
        publisher,
        mut work,
        ..
    } = *job;
    let result = command.perform(db, route);
    turn.settle(shared, Some(&mut work));
    work.queue_wait_ns = wait;
    work.service_ns = elapsed(start);
    shared.progress(namespace, class, wait, work.service_ns);
    publisher.publish(Box::new(Outcome { result, work }));
}
fn run(shared: &Shared) {
    let mut cursor = 0_u64;
    let mut maintenance_failed = false;
    let mut live_cursor = layerfs_overlay::MaintenanceCursor::default();
    let mut live_turn = true;
    while let Some((job, event, due)) = shared.poll() {
        let mut maintained = false;
        let idle = job.is_none();
        {
            let connection = shared.connection();
            let Some(db) = connection.as_deref() else {
                drop(connection);
                if let Some(job) = job {
                    job.refuse(OwnerError::Stopped);
                }
                shared.idle(false, true);
                return;
            };
            if !maintenance_failed && due {
                let turn = Turn::begin(db);
                let start = Instant::now();
                let maintenance =
                    maintenance_turn(db, &mut live_cursor, &mut cursor, &mut live_turn);
                turn.settle(shared, None);
                match maintenance {
                    Ok(Some((step, closed))) => {
                        shared.maintenance(step, closed, elapsed(start));
                        maintained = true;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        maintenance_failed = true;
                        shared.maintenance_failed(error);
                    }
                }
            }
            if let Some(job) = job {
                serve(shared, db, job);
            }
        }
        shared.idle(false, true);
        if idle && !maintained {
            shared.wait(event);
        }
    }
}

fn maintenance_turn(
    db: &Overlay,
    live: &mut layerfs_overlay::MaintenanceCursor,
    closed: &mut u64,
    live_turn: &mut bool,
) -> Result<Option<(layerfs_overlay::ReclaimStep, bool)>, OverlayError> {
    let first = *live_turn;
    *live_turn = !*live_turn;
    for use_live in [first, !first] {
        if use_live {
            if let Some(step) = db.maintain(*live)? {
                *live = step.cursor;
                return Ok(Some((step.work, false)));
            }
        } else if let Some(step) = db.reclaim_closed(*closed)? {
            *closed = step.namespace;
            return Ok(Some((step, true)));
        }
    }
    Ok(None)
}
