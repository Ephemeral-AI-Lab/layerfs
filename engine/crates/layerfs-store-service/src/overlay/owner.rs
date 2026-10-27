//! Calling-thread jobs under one Workspace connection lock.
use super::state::{elapsed, Shared};
use crate::{JobWork, OwnerWork, ServiceClass};
use layerfs_overlay::{DatabaseProfile, Overlay, OverlayError, ProfileConfig};
use layerfs_workspace::Route;
use std::{
    fmt, io,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};
#[derive(Clone, Copy, Debug)]
pub struct OwnerConfig {
    pub namespaces: usize,
}
impl Default for OwnerConfig {
    fn default() -> Self {
        Self { namespaces: 16 }
    }
}
#[derive(Debug)]
pub enum OwnerError {
    Io(io::Error),
    /// Failed unlink keeps the original file identity handle, not just its path.
    Deletion {
        cause: io::Error,
        backing: std::fs::File,
    },
    Overlay(OverlayError),
    Stopped,
    Retained(Arc<OwnerError>),
    Attempted(Box<WorkspaceCallFailure>),
    AdmissionFull,
    InvalidAdmission,
    IdentityExhausted,
    /// Original join payload. The mutex preserves Sync error custody even
    /// when the caller's original panic value is Send but not Sync.
    WorkspacePanicked(WorkspacePanic),
    Unattempted {
        cause: Box<OwnerError>,
        input: OriginalInput,
    },
    Install {
        cause: Box<layerfs_workspace::WorkspaceError>,
        input: Box<layerfs_workspace::PreparedBase>,
    },
    /// Exact failure of an attempted namespace job; never replayed.
    Workspace(Box<layerfs_workspace::WorkspaceError>),
}
/// Original attempted failure and the backing it may still own.
pub struct WorkspaceCallFailure {
    pub cause: Box<OwnerError>,
    pub work: JobWork,
    _backing: Option<Arc<super::backing::Backing>>,
}
impl fmt::Debug for WorkspaceCallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)
    }
}
pub struct WorkspacePanic {
    pub payload: Mutex<Box<dyn std::any::Any + Send>>,
    // A panic establishes neither rollback nor a checked close. Keep the exact
    // connection and memory state in the failure, with no implicit replay/drop.
    _connection: Mutex<std::mem::ManuallyDrop<Box<Overlay>>>,
}
impl fmt::Debug for WorkspacePanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Workspace panic and original connection retained")
    }
}
pub struct OriginalInput(Mutex<Box<dyn std::any::Any + Send>>);
impl OriginalInput {
    pub(super) fn new<T: Send + 'static>(input: T) -> Self {
        Self(Mutex::new(Box::new(input)))
    }
    pub fn inspect<T: 'static, R>(&self, inspect: impl FnOnce(Option<&T>) -> R) -> R {
        let input = self.0.lock().unwrap_or_else(|e| e.into_inner());
        inspect(input.downcast_ref())
    }
}
impl fmt::Debug for OriginalInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("original input retained")
    }
}
#[derive(Clone, Copy)]
pub enum Readiness {
    Capture,
    Install(layerfs_overlay::Capture),
}
impl fmt::Display for OwnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OwnerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Retained(error) => Some(error.as_ref()),
            Self::Attempted(error) => Some(error.cause.as_ref()),
            Self::Io(error) => Some(error),
            Self::Deletion { cause, .. } => Some(cause),
            Self::Overlay(error) => Some(error),
            Self::Unattempted { cause, .. } => Some(cause.as_ref()),
            Self::Install { cause, .. } => Some(cause.as_ref()),
            Self::Workspace(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}

pub struct Owner {
    pub(super) client: OwnerClient,
    pub(super) profile: DatabaseProfile,
}
#[derive(Clone)]
pub struct OwnerClient {
    pub(super) context: Context,
}
#[derive(Clone)]
pub(super) enum Context {
    Roster(Arc<super::workspaces::Workspaces>),
    Workspace(Arc<Shared>),
}
impl Owner {
    pub fn start(
        path: &Path,
        profile: ProfileConfig,
        config: OwnerConfig,
    ) -> Result<Self, OwnerError> {
        Self::start_observed(path, profile, config).result
    }
    pub fn start_observed(
        path: &Path,
        profile: ProfileConfig,
        config: OwnerConfig,
    ) -> crate::OwnerStart {
        super::workspaces::start(path, profile, config)
    }
    pub(super) fn start_copy(
        path: &Path,
        template: &Path,
        profile: ProfileConfig,
        shared: Arc<Shared>,
    ) -> crate::OwnerStart {
        let started = Instant::now();
        let created = Overlay::copy_template_observed(path, profile, template);
        let startup = Arc::new(created.work);
        let result = created.result.map_err(OwnerError::Overlay).map(|db| {
            let profile = db.profile().clone();
            {
                let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
                state.db = Some(Box::new(db));
                state.initial = Some(startup.clone());
            }
            Self {
                client: OwnerClient {
                    context: Context::Workspace(shared),
                },
                profile,
            }
        });
        crate::OwnerStart {
            result,
            startup,
            creation_reported: true,
            elapsed_ns: elapsed(started),
        }
    }
    pub fn configuration(&self) -> OwnerConfig {
        self.client
            .roster()
            .map_or(OwnerConfig { namespaces: 1 }, |pool| pool.configuration())
    }
    pub fn client(&self) -> OwnerClient {
        self.client.clone()
    }
    pub fn profile(&self) -> &DatabaseProfile {
        &self.profile
    }
    pub fn stop(self) -> Result<(), OwnerError> {
        if let Some(pool) = self.client.roster() {
            pool.stop()
        } else {
            self.client.close_connection()
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(pool) = self.client.roster() {
            let _ = pool.stop();
        } else {
            let _ = self.client.close_connection();
        }
    }
}
impl OwnerClient {
    pub(super) fn roster(&self) -> Option<&Arc<super::workspaces::Workspaces>> {
        match &self.context {
            Context::Roster(pool) => Some(pool),
            Context::Workspace(_) => None,
        }
    }
    pub(super) fn connection(&self) -> Result<&Arc<Shared>, OwnerError> {
        match &self.context {
            Context::Workspace(shared) => Ok(shared),
            Context::Roster(_) => Err(OwnerError::InvalidAdmission),
        }
    }

    /// One short operation; the closure performs at most one transaction and
    /// does no provider I/O. Before-effect failure retains the original input.
    pub fn call<T, F>(
        &self,
        route: Route,
        class: ServiceClass,
        ready: Option<Readiness>,
        action: F,
    ) -> Result<T, OwnerError>
    where
        F: FnOnce(&Overlay) -> layerfs_overlay::OverlayResult<T> + Send + 'static,
    {
        self.call_observed(Some(route), class, ready, move |db, _| {
            action(db).map_err(OwnerError::Overlay)
        })
        .map(|(value, _)| value)
    }
    pub fn call_observed<T, F>(
        &self,
        route: Option<Route>,
        class: ServiceClass,
        ready: Option<Readiness>,
        action: F,
    ) -> Result<(T, JobWork), OwnerError>
    where
        F: FnOnce(&Overlay, Option<Route>) -> Result<T, OwnerError> + Send + 'static,
    {
        if let Some(pool) = self.roster() {
            let client = route
                .ok_or(OwnerError::InvalidAdmission)
                .and_then(|route| pool.client(route.namespace()));
            return match client {
                Ok(client) => client.call_observed(route, class, ready, action),
                Err(cause) => Err(OwnerError::Unattempted {
                    cause: Box::new(cause),
                    input: OriginalInput::new(action),
                }),
            };
        }
        let refuse = |cause, action| OwnerError::Unattempted {
            cause: Box::new(cause),
            input: OriginalInput::new(action),
        };
        let shared = match self.connection() {
            Ok(shared) => shared,
            Err(cause) => return Err(refuse(cause, action)),
        };
        if route.is_some_and(|route| shared.route.get() != Some(&route)) {
            return Err(refuse(OwnerError::Overlay(OverlayError::Stale), action));
        }
        let requested = Instant::now();
        let mut state = match shared.state.lock() {
            Ok(state) => state,
            Err(_) => return Err(refuse(OwnerError::Stopped, action)),
        };
        let mut wait = elapsed(requested);
        // A waiting Capture or known install owns a finite frontier: mutations
        // and source acquisitions that arrive later wait behind it here, before
        // any effect, so sustained traffic cannot keep extending what it awaits.
        let mut fronted = false;
        // Readiness checks already recorded as their own jobs; the receipt of
        // the call that ran them reports them with its final job.
        let mut checks = JobWork::default();
        let (sql, allocation, payload, start) = loop {
            let refused = state
                .failure
                .as_ref()
                .map(|cause| OwnerError::Retained(cause.clone()))
                .or_else(|| (state.stopping || state.db.is_none()).then_some(OwnerError::Stopped));
            if let Some(cause) = refused {
                // Refused before any effect: nothing ran, so no job is counted.
                state.front(ready, &mut fronted, false);
                drop(state);
                shared.changed.notify_all();
                return Err(refuse(cause, action));
            }
            if state.behind_frontier(class) {
                let waiting = Instant::now();
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
                wait = wait.saturating_add(elapsed(waiting));
                continue;
            }
            let db = state.db.as_deref().expect("live connection");
            let start = Instant::now();
            let sql = db.diagnostics();
            let allocation = db.allocation_work();
            let payload = db.payload_work();
            let observed_ready = match ready {
                Some(Readiness::Capture) => {
                    // Composition does not fence later mutations.
                    db.capture_ready().and_then(|ready| {
                        Ok(if ready {
                            Ok(())
                        } else {
                            Err(db.state()?.consolidating.is_none())
                        })
                    })
                }
                Some(Readiness::Install(capture)) => {
                    db.install_ready(capture)
                        .map(|ready| if ready { Ok(()) } else { Err(true) })
                }
                None => Ok(Ok(())),
            };
            if matches!(observed_ready, Ok(Ok(()))) {
                state.front(ready, &mut fronted, false);
                break (sql, allocation, payload, start);
            }
            let mut work = JobWork {
                jobs: 1,
                ..JobWork::default()
            };
            work.sql.accumulate(db.diagnostics().since(&sql));
            work.allocation = db.allocation_work().since(allocation);
            work.payload = db.payload_work().since(payload);
            work.lock_wait_ns = wait;
            work.hold_ns = elapsed(start);
            state.record(class, &work, false);
            checks.jobs += 1;
            checks.sql.accumulate(work.sql);
            checks.allocation.accumulate(work.allocation);
            checks.payload.accumulate(work.payload);
            checks.lock_wait_ns = checks.lock_wait_ns.saturating_add(work.lock_wait_ns);
            checks.hold_ns = checks.hold_ns.saturating_add(work.hold_ns);
            let frontier = match observed_ready {
                Ok(frontier) => frontier.is_err_and(|frontier| frontier),
                Err(cause) => {
                    state.front(ready, &mut fronted, false);
                    drop(state);
                    shared.changed.notify_all();
                    return Err(refuse(OwnerError::Overlay(cause), action));
                }
            };
            if state.front(ready, &mut fronted, frontier) {
                shared.changed.notify_all();
            }
            let waiting = Instant::now();
            state = shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
            wait = elapsed(waiting);
        };
        let db = state.db.as_deref().expect("open connection");
        let performed = catch_unwind(AssertUnwindSafe(|| action(db, route)));
        let mut work = JobWork {
            jobs: 1,
            ..JobWork::default()
        };
        work.sql.accumulate(db.diagnostics().since(&sql));
        work.allocation = db.allocation_work().since(allocation);
        work.payload = db.payload_work().since(payload);
        work.lock_wait_ns = wait;
        work.hold_ns = elapsed(start);
        // A quarantined engine can run no later maintenance step: that stop
        // is retained here, with no statement and no job.
        let quarantined = db.quarantined();
        let needs_maintenance = !quarantined && db.maintenance_pending();
        let disposals = db.take_open_disposals();
        if quarantined && state.maintenance_error.is_none() {
            state.maintenance_error = Some(Arc::new(OverlayError::Quarantined));
        }
        if let Some(initial) = state.initial.take() {
            work.sql.accumulate(initial.sql);
            work.allocation.accumulate(initial.allocation);
        }
        let result = match performed {
            Ok(result) => result,
            Err(panic) => {
                let cause = retain_panic(&mut state, panic);
                Err(OwnerError::Retained(cause))
            }
        };
        state.record(class, &work, false);
        work.jobs += checks.jobs;
        work.sql.accumulate(checks.sql);
        work.allocation.accumulate(checks.allocation);
        work.payload.accumulate(checks.payload);
        work.lock_wait_ns = work.lock_wait_ns.saturating_add(checks.lock_wait_ns);
        work.hold_ns = work.hold_ns.saturating_add(checks.hold_ns);
        let backing = state.backing.clone();
        drop(state);
        shared.changed.notify_all();
        drop(disposals);
        if needs_maintenance {
            self.maintain();
        }
        match result {
            Ok(value) => Ok((value, work)),
            Err(cause) => Err(OwnerError::Attempted(Box::new(WorkspaceCallFailure {
                cause: Box::new(cause),
                work,
                _backing: backing,
            }))),
        }
    }
    fn maintain(&self) {
        let Ok(shared) = self.connection() else {
            return;
        };
        loop {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.stopping || state.failure.is_some() || state.maintenance_error.is_some() {
                return;
            }
            let Some(db) = state.db.as_deref() else {
                return;
            };
            if !db.maintenance_pending() {
                return;
            }
            let start = Instant::now();
            let sql = db.diagnostics();
            let allocation = db.allocation_work();
            let payload = db.payload_work();
            let step = catch_unwind(AssertUnwindSafe(|| db.maintain(state.cursor)));
            let sql = db.diagnostics().since(&sql);
            let allocation = db.allocation_work().since(allocation);
            let payload = db.payload_work().since(payload);
            let disposals = db.take_open_disposals();
            let mut work = JobWork {
                jobs: 1,
                hold_ns: elapsed(start),
                ..JobWork::default()
            };
            work.sql.accumulate(sql);
            work.allocation = allocation;
            work.payload = payload;
            state.record(ServiceClass::General, &work, true);
            let progress = match step {
                Ok(Ok(Some(step))) => {
                    state.cursor = step.cursor;
                    state.work.maintenance_rows =
                        state.work.maintenance_rows.saturating_add(step.work.rows);
                    state.work.maintenance_data_bytes = state
                        .work
                        .maintenance_data_bytes
                        .saturating_add(step.work.data_bytes);
                    true
                }
                Ok(Ok(None)) => false,
                Ok(Err(cause)) => {
                    state.maintenance_error = Some(Arc::new(cause));
                    false
                }
                Err(payload) => {
                    retain_panic(&mut state, payload);
                    false
                }
            };
            drop(state);
            drop(disposals);
            shared.changed.notify_all();
            if !progress {
                return;
            }
        }
    }
    fn close_connection(&self) -> Result<(), OwnerError> {
        let shared = self.connection()?;
        let requested = Instant::now();
        let mut state = shared.state.lock().map_err(|_| OwnerError::Stopped)?;
        let wait = elapsed(requested);
        let start = Instant::now();
        let result = if let Some(cause) = &state.failure {
            Err(OwnerError::Retained(cause.clone()))
        } else {
            state.stopping = true;
            match state.db.take() {
                Some(db) => match db.close_database() {
                    Ok(()) => {
                        if let Some(backing) = &state.backing {
                            backing.closed();
                        }
                        Ok(())
                    }
                    Err(cause) => {
                        let cause = Arc::new(OwnerError::Overlay(cause));
                        state.failure = Some(cause.clone());
                        Err(OwnerError::Retained(cause))
                    }
                },
                None => Ok(()),
            }
        };
        let work = JobWork {
            jobs: 1,
            lock_wait_ns: wait,
            hold_ns: elapsed(start),
            ..JobWork::default()
        };
        state.record(ServiceClass::General, &work, false);
        drop(state);
        shared.changed.notify_all();
        result
    }
    pub fn bound_route(&self) -> Result<Route, OwnerError> {
        self.connection()?
            .route
            .get()
            .copied()
            .ok_or(OwnerError::InvalidAdmission)
    }
    pub fn workspace_client(&self, route: Route) -> Result<Self, OwnerError> {
        match self.roster() {
            Some(pool) => pool.client(route.namespace()),
            None => Ok(self.clone()),
        }
    }
    pub fn retire_namespace(&self, namespace: i64) -> Result<(), OwnerError> {
        self.roster()
            .ok_or(OwnerError::InvalidAdmission)?
            .retire(namespace)
    }
    /// One short hold of the Workspace lock with no SQL: the reply ticket
    /// validates engine, incarnation, generation and revision, and the release
    /// wakes a waiter. Counted as a reply attempt, never as a job of a class.
    pub fn reply_attempted(
        &self,
        publication: layerfs_overlay::Publication,
    ) -> Result<(), OwnerError> {
        let client = self.clone();
        let shared = client.connection()?;
        let mut state = shared.state.lock().map_err(|_| OwnerError::Stopped)?;
        let start = Instant::now();
        if let Some(cause) = &state.failure {
            return Err(OwnerError::Retained(cause.clone()));
        }
        let Some(db) = state.db.as_deref().filter(|_| !state.stopping) else {
            return Err(OwnerError::Stopped);
        };
        let result = db
            .reply_tickets()
            .attempted(publication)
            .map(|_| ())
            .map_err(OwnerError::Overlay);
        let needs_maintenance = db.maintenance_pending();
        let disposals = db.take_open_disposals();
        state.work.reply_attempts = state.work.reply_attempts.saturating_add(1);
        state.work.reply_hold_ns = state.work.reply_hold_ns.saturating_add(elapsed(start));
        let backing = state.backing.clone();
        drop(state);
        shared.changed.notify_all();
        drop(disposals);
        if needs_maintenance {
            client.maintain();
        }
        result.map_err(|cause| {
            OwnerError::Attempted(Box::new(WorkspaceCallFailure {
                cause: Box::new(cause),
                work: JobWork::default(),
                _backing: backing,
            }))
        })
    }
    pub fn diagnostics(&self) -> Result<OwnerWork, OwnerError> {
        match self.roster() {
            Some(pool) => pool.diagnostics(),
            None => {
                let mut state = self
                    .connection()?
                    .state
                    .lock()
                    .map_err(|_| OwnerError::Stopped)?;
                state.work.observations = state.work.observations.saturating_add(1);
                Ok(state.work)
            }
        }
    }
    pub fn maintenance_failure(&self) -> Result<Option<Arc<OverlayError>>, OwnerError> {
        if let Some(pool) = self.roster() {
            return pool.maintenance_failure();
        }
        let mut state = self
            .connection()?
            .state
            .lock()
            .map_err(|_| OwnerError::Stopped)?;
        state.work.observations = state.work.observations.saturating_add(1);
        match &state.failure {
            Some(cause) => Err(OwnerError::Retained(cause.clone())),
            None => Ok(state.maintenance_error.clone()),
        }
    }
}

fn retain_panic(
    state: &mut super::state::State,
    payload: Box<dyn std::any::Any + Send>,
) -> Arc<OwnerError> {
    let db = state
        .db
        .take()
        .expect("panic follows connection acquisition");
    let cause = Arc::new(OwnerError::WorkspacePanicked(WorkspacePanic {
        payload: Mutex::new(payload),
        _connection: Mutex::new(std::mem::ManuallyDrop::new(db)),
    }));
    state.failure = Some(cause.clone());
    cause
}
