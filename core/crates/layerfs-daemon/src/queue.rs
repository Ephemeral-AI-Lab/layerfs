//! Bounded scheduling state, independent of authoritative filesystem metadata.
use crate::{
    commands::{Command, Response, ServiceClass},
    credits::Credit,
    owner::OwnerError,
};
use layerfs_overlay::Route;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{mpsc::SyncSender, Arc, Condvar, Mutex},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct OwnerWork {
    pub admitted: u64,
    pub completed: [u64; 6],
    pub queue_wait_ns: [u64; 6],
    pub service_ns: [u64; 6],
    pub credited_bytes: usize,
    pub peak_credited_bytes: usize,
    pub outstanding: usize,
    pub maintenance_jobs: u64,
    pub maintenance_rows: u64,
    pub maintenance_data_bytes: u64,
    pub maintenance_ns: u64,
}
pub(crate) struct Envelope {
    pub result: Result<Response, OwnerError>,
    pub _credit: Arc<Credit>,
}
pub(crate) struct Job {
    pub id: u64,
    pub route: Option<Route>,
    pub command: Command,
    pub credit: Arc<Credit>,
    pub reply: SyncSender<Envelope>,
    pub admitted: Instant,
    pub blocked: bool,
}
pub(crate) struct Lane {
    pub queues: [VecDeque<Job>; 6],
    pub next: usize,
    pub ordinary: usize,
    pub lifecycle: usize,
}
impl Lane {
    pub fn new() -> Self {
        Self {
            queues: std::array::from_fn(|_| VecDeque::new()),
            next: 0,
            ordinary: 0,
            lifecycle: 0,
        }
    }
    fn take(&mut self) -> Option<Job> {
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
                    .any(|job| matches!(job.command, Command::Capture) && job.id < front.id)
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
    pub lanes: BTreeMap<i64, Lane>,
    pub rotation: VecDeque<i64>,
    pub stopping: bool,
    pub next: u64,
    pub work: OwnerWork,
    pub event: u64,
    pub maintenance_error: Option<Arc<layerfs_overlay::OverlayError>>,
}
pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub wake: Condvar,
    pub config: crate::owner::OwnerConfig,
}
impl Shared {
    pub fn new(config: crate::owner::OwnerConfig) -> Self {
        Self {
            state: Mutex::new(State {
                lanes: BTreeMap::new(),
                rotation: VecDeque::new(),
                stopping: false,
                next: 1,
                work: OwnerWork::default(),
                event: 0,
                maintenance_error: None,
            }),
            wake: Condvar::new(),
            config,
        }
    }
    pub fn poll(&self) -> Option<(Option<Job>, u64)> {
        let mut state = self.state.lock().ok()?;
        if state.stopping {
            return None;
        }
        for _ in 0..state.rotation.len() {
            let ns = state.rotation.pop_front()?;
            state.rotation.push_back(ns);
            if let Some(job) = state.lanes.get_mut(&ns)?.take() {
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
    pub fn maintenance(&self, step: layerfs_overlay::ReclaimStep, ns: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.work.maintenance_jobs = state.work.maintenance_jobs.saturating_add(1);
            state.work.maintenance_rows = state.work.maintenance_rows.saturating_add(step.rows);
            state.work.maintenance_data_bytes = state
                .work
                .maintenance_data_bytes
                .saturating_add(step.data_bytes);
            state.work.maintenance_ns = state.work.maintenance_ns.saturating_add(ns);
        }
    }
    pub fn maintenance_failed(&self, error: layerfs_overlay::OverlayError) {
        if let Ok(mut state) = self.state.lock() {
            state.maintenance_error = Some(Arc::new(error));
        }
    }
    pub fn stopped(&self) -> bool {
        self.state.lock().map_or(true, |state| state.stopping)
    }
    pub fn park(&self, mut job: Job) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                let _ = job.reply.send(Envelope {
                    result: Err(OwnerError::Unattempted {
                        cause: Box::new(OwnerError::Stopped),
                        command: Box::new(job.command),
                    }),
                    _credit: job.credit,
                });
                return;
            }
        };
        if state.stopping {
            drop(state);
            let _ = job.reply.send(Envelope {
                result: Err(OwnerError::Unattempted {
                    cause: Box::new(OwnerError::Stopped),
                    command: Box::new(job.command),
                }),
                _credit: job.credit,
            });
            return;
        }
        let Some(lane) = state.lanes.get_mut(&job.route.map_or(0, Route::namespace)) else {
            drop(state);
            let _ = job.reply.send(Envelope {
                result: Err(OwnerError::Unattempted {
                    cause: Box::new(OwnerError::Stopped),
                    command: Box::new(job.command),
                }),
                _credit: job.credit,
            });
            return;
        };
        job.blocked = true;
        lane.queues[job.command.class() as usize].push_front(job);
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
                if let Some(lane) = state.lanes.get_mut(&ns) {
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
            state
                .lanes
                .values_mut()
                .flat_map(|lane| lane.queues.iter_mut().flat_map(|q| q.drain(..)))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        // Drop/send retained credits outside the queue lock.
        for job in jobs {
            let _ = job.reply.send(Envelope {
                result: Err(OwnerError::Unattempted {
                    cause: Box::new(OwnerError::Stopped),
                    command: Box::new(job.command),
                }),
                _credit: job.credit,
            });
        }
        self.wake.notify_all();
    }
}
