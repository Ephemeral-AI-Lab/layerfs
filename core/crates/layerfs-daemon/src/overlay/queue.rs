//! Bounded scheduling state, independent of authoritative filesystem metadata.
use crate::{
    commands::{Command, ServiceClass},
    owner::{OwnerConfig, OwnerError},
    service::completion::{Outcome, Publisher},
};
use layerfs_overlay::Route;
use std::{
    collections::VecDeque,
    mem::size_of,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct OwnerWork {
    /// Exclusive command jobs; automatic maintenance is accounted separately.
    pub sql_foreground: layerfs_overlay::DatabaseWork,
    pub sql_maintenance: layerfs_overlay::DatabaseWork,
    pub payload_foreground: layerfs_overlay::PayloadWork,
    pub payload_maintenance: layerfs_overlay::PayloadWork,
    pub allocation_foreground: layerfs_overlay::AllocationWork,
    pub allocation_maintenance: layerfs_overlay::AllocationWork,
    pub admitted: u64,
    pub completed: [u64; 6],
    pub queue_wait_ns: [u64; 6],
    pub service_ns: [u64; 6],
    pub credited_bytes: usize,
    pub peak_credited_bytes: usize,
    pub outstanding: usize,
    /// Fixed scheduler state and actual lane queue capacity, allocated once at
    /// startup and excluded from the admission bytes available to jobs.
    pub scheduler_bytes: usize,
    /// Admitted jobs waiting in a lane, including parked ones; excludes the
    /// executing job and caller-held results.
    pub queued: usize,
    pub peak_queued: usize,
    /// Jobs whose retained receipt rows exceeded their admitted allowance, and
    /// the bytes charged for them when observed.
    pub receipt_overruns: u64,
    pub receipt_overrun_bytes: usize,
    pub maintenance_jobs: u64,
    pub maintenance_rows: u64,
    /// Acknowledged terminal namespace reclamations; neither logical Close nor
    /// a completed live-maintenance step increments this observation.
    pub closed_namespaces: u64,
    pub maintenance_data_bytes: u64,
    pub maintenance_ns: u64,
}
pub(crate) struct Job {
    pub id: u64,
    pub route: Option<Route>,
    pub command: Command,
    pub publisher: Publisher,
    pub admitted: Instant,
    pub blocked: bool,
    pub wait_consolidation: bool,
    pub work: crate::JobWork,
}
impl Job {
    /// Returns the original command with any earlier readiness receipt. The
    /// job's storage is released before its outcome is allocated.
    pub fn refuse(self: Box<Self>, cause: OwnerError) {
        let Job {
            command,
            publisher,
            work,
            ..
        } = *self;
        publisher.publish(Box::new(Outcome {
            result: Err(OwnerError::Unattempted {
                cause: Box::new(cause),
                command: Box::new(command),
            }),
            work,
        }));
    }
}
/// One namespace's admitted jobs. Queues hold job pointers in capacity fixed at
/// startup; a lane is reused once its last credit is released.
pub(crate) struct Lane {
    pub namespace: i64,
    pub active: bool,
    pub queues: [VecDeque<Box<Job>>; 6],
    pub next: usize,
    pub ordinary: usize,
    pub lifecycle: usize,
}
impl Lane {
    fn new(config: OwnerConfig) -> Self {
        Self {
            namespace: 0,
            active: false,
            queues: std::array::from_fn(|class| {
                VecDeque::with_capacity(if class == ServiceClass::Lifecycle as usize {
                    config.lifecycle_jobs_per_namespace
                } else {
                    config.jobs_per_namespace
                })
            }),
            next: 0,
            ordinary: 0,
            lifecycle: 0,
        }
    }
    fn take(&mut self) -> Option<Box<Job>> {
        for _ in 0..6 {
            let index = self.next;
            self.next = (self.next + 1) % 6;
            let Some(front) = self.queues[index].front() else {
                continue;
            };
            if front.blocked {
                continue;
            }
            // A known install waits only for its finite earlier base-source
            // frontier. New acquisitions park outside the SQL attempt; existing
            // reads/releases use other classes and remain runnable.
            if index == ServiceClass::Source as usize
                && self.queues[ServiceClass::Capture as usize]
                    .iter()
                    .any(|job| job.command.install_capture().is_some() && job.id < front.id)
            {
                continue;
            }
            if front.command.install_capture().is_some()
                && self.queues[ServiceClass::Source as usize]
                    .iter()
                    .any(|job| job.id < front.id)
            {
                continue;
            }
            // Drain only the finite pre-capture mutation/reply frontier. Later
            // writes cannot keep extending its pending-publication domain.
            if index == ServiceClass::Mutation as usize
                && self.queues[ServiceClass::Capture as usize]
                    .iter()
                    .any(|job| {
                        matches!(job.command, Command::Capture)
                            && !job.wait_consolidation
                            && job.id < front.id
                    })
            {
                continue;
            }
            if matches!(front.command, Command::Capture)
                && self.queues[ServiceClass::Mutation as usize]
                    .iter()
                    .any(|job| job.id < front.id)
            {
                continue;
            }
            return self.queues[index].pop_front();
        }
        None
    }
}
pub(crate) struct State {
    pub lanes: Vec<Lane>,
    pub rotation: VecDeque<i64>,
    pub stopping: bool,
    pub next: u64,
    pub work: OwnerWork,
    pub event: u64,
    pub maintenance_error: Option<Arc<layerfs_overlay::OverlayError>>,
}
impl State {
    pub fn lane(&mut self, namespace: i64) -> Option<&mut Lane> {
        self.lanes
            .iter_mut()
            .find(|lane| lane.active && lane.namespace == namespace)
    }
    pub fn queued(&mut self) {
        self.work.queued += 1;
        self.work.peak_queued = self.work.peak_queued.max(self.work.queued);
    }
    /// Returns one job's slot; its lane leaves the rotation with its last job.
    pub fn release(&mut self, namespace: i64, class: ServiceClass) {
        let Some(lane) = self.lane(namespace) else {
            return;
        };
        if class == ServiceClass::Lifecycle {
            lane.lifecycle -= 1;
        } else {
            lane.ordinary -= 1;
        }
        if lane.lifecycle == 0 && lane.ordinary == 0 {
            lane.active = false;
            self.rotation.retain(|ns| *ns != namespace);
        }
    }
}
pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub wake: Condvar,
    pub config: OwnerConfig,
}
impl Shared {
    /// Startup scheduler bytes for a configuration, before any is allocated.
    pub fn planned_bytes(config: OwnerConfig) -> Option<usize> {
        let pointers = config
            .jobs_per_namespace
            .checked_mul(5)?
            .checked_add(config.lifecycle_jobs_per_namespace)?
            .checked_mul(size_of::<Box<Job>>())?;
        pointers
            .checked_add(size_of::<Lane>() + size_of::<i64>())?
            .checked_mul(config.namespaces)?
            .checked_add(2 * size_of::<usize>() + size_of::<Self>())
    }
    pub fn sql_progress(
        &self,
        work: layerfs_overlay::DatabaseWork,
        allocation: layerfs_overlay::AllocationWork,
        payload: layerfs_overlay::PayloadWork,
        maintenance: bool,
    ) {
        if let Ok(mut state) = self.state.lock() {
            if maintenance {
                state.work.sql_maintenance.accumulate(work);
                state.work.payload_maintenance.accumulate(payload);
                state.work.allocation_maintenance.accumulate(allocation);
            } else {
                state.work.sql_foreground.accumulate(work);
                state.work.payload_foreground.accumulate(payload);
                state.work.allocation_foreground.accumulate(allocation);
            }
        }
    }
    pub fn new(config: OwnerConfig) -> Self {
        let mut lanes = Vec::with_capacity(config.namespaces);
        lanes.resize_with(config.namespaces, || Lane::new(config));
        let rotation = VecDeque::with_capacity(config.namespaces);
        // Actual capacities, which an allocator may round above the request.
        let scheduler_bytes = 2 * size_of::<usize>()
            + size_of::<Self>()
            + lanes.capacity() * size_of::<Lane>()
            + rotation.capacity() * size_of::<i64>()
            + lanes
                .iter()
                .flat_map(|lane| &lane.queues)
                .map(|queue| queue.capacity() * size_of::<Box<Job>>())
                .sum::<usize>();
        Self {
            state: Mutex::new(State {
                lanes,
                rotation,
                stopping: false,
                next: 1,
                work: OwnerWork {
                    scheduler_bytes,
                    ..OwnerWork::default()
                },
                event: 0,
                maintenance_error: None,
            }),
            wake: Condvar::new(),
            config,
        }
    }
    pub fn scheduler_bytes(&self) -> usize {
        self.state
            .lock()
            .map_or(usize::MAX, |state| state.work.scheduler_bytes)
    }
    pub fn poll(&self) -> Option<(Option<Box<Job>>, u64)> {
        let mut state = self.state.lock().ok()?;
        if state.stopping {
            return None;
        }
        for _ in 0..state.rotation.len() {
            let ns = state.rotation.pop_front()?;
            state.rotation.push_back(ns);
            if let Some(job) = state.lane(ns)?.take() {
                state.work.queued -= 1;
                return Some((Some(job), state.event));
            }
        }
        Some((None, state.event))
    }
    pub fn wait(&self, event: u64) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        while !state.stopping && state.event == event {
            let Ok(next) = self.wake.wait(state) else {
                return;
            };
            state = next;
        }
    }
    pub fn maintenance(&self, step: layerfs_overlay::ReclaimStep, closed: bool, ns: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.event = state.event.wrapping_add(1);
            if let Some(lane) = state.lane(step.namespace as i64) {
                for job in &mut lane.queues[ServiceClass::Capture as usize] {
                    job.blocked = false;
                }
            }
            state.work.maintenance_jobs = state.work.maintenance_jobs.saturating_add(1);
            if closed && step.done {
                state.work.closed_namespaces = state.work.closed_namespaces.saturating_add(1);
            }
            state.work.maintenance_rows = state.work.maintenance_rows.saturating_add(step.rows);
            state.work.maintenance_data_bytes = state
                .work
                .maintenance_data_bytes
                .saturating_add(step.data_bytes);
            state.work.maintenance_ns = state.work.maintenance_ns.saturating_add(ns);
        }
        self.wake.notify_all();
    }
    pub fn maintenance_failed(&self, error: layerfs_overlay::OverlayError) {
        if let Ok(mut state) = self.state.lock() {
            state.maintenance_error = Some(Arc::new(error));
        }
    }
    pub fn stopped(&self) -> bool {
        self.state.lock().map_or(true, |state| state.stopping)
    }
    /// Requeues a job whose readiness check found it not yet runnable. A job
    /// which can no longer wait gets its original command back unattempted.
    pub fn park(&self, mut job: Box<Job>) {
        if let Ok(mut state) = self.state.lock() {
            let stopping = state.stopping;
            let namespace = job.route.map_or(0, Route::namespace);
            if let Some(lane) = state.lane(namespace).filter(|_| !stopping) {
                job.work.parked_turns = job.work.parked_turns.saturating_add(1);
                job.blocked = true;
                lane.queues[job.command.class() as usize].push_front(job);
                state.queued();
                return;
            }
        }
        // The outcome and its credit are released outside the queue lock.
        job.refuse(OwnerError::Stopped);
    }
    pub fn progress(&self, ns: i64, class: ServiceClass, wait: u64, service: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.event = state.event.wrapping_add(1);
            let work = &mut state.work;
            work.completed[class as usize] = work.completed[class as usize].saturating_add(1);
            work.queue_wait_ns[class as usize] =
                work.queue_wait_ns[class as usize].saturating_add(wait);
            work.service_ns[class as usize] =
                work.service_ns[class as usize].saturating_add(service);
            if matches!(class, ServiceClass::Mutation | ServiceClass::Lifecycle) {
                if let Some(lane) = state.lane(ns) {
                    for job in &mut lane.queues[ServiceClass::Capture as usize] {
                        job.blocked = false;
                    }
                }
            }
        }
        self.wake.notify_all();
    }
    pub fn stop(&self) {
        let jobs = if let Ok(mut state) = self.state.lock() {
            state.stopping = true;
            state.event = state.event.wrapping_add(1);
            state.work.queued = 0;
            state
                .lanes
                .iter_mut()
                .flat_map(|lane| lane.queues.iter_mut().flat_map(|q| q.drain(..)))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        // Publish outcomes and drop retained credits outside the queue lock.
        for job in jobs {
            job.refuse(OwnerError::Stopped);
        }
        self.wake.notify_all();
    }
}
