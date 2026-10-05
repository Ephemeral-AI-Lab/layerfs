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
    pub completed: [u64; 5],
    pub queue_wait_ns: [u64; 5],
    pub service_ns: [u64; 5],
    pub credited_bytes: usize,
    pub peak_credited_bytes: usize,
    pub outstanding: usize,
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
    pub queues: [VecDeque<Job>; 5],
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
        for _ in 0..5 {
            let index = self.next;
            self.next = (self.next + 1) % 5;
            let Some(front) = self.queues[index].front() else {
                continue;
            };
            if front.blocked {
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
            }),
            wake: Condvar::new(),
            config,
        }
    }
    pub fn take(&self) -> Option<Job> {
        let mut state = self.state.lock().ok()?;
        loop {
            if state.stopping {
                return None;
            }
            for _ in 0..state.rotation.len() {
                let ns = state.rotation.pop_front()?;
                state.rotation.push_back(ns);
                if let Some(job) = state.lanes.get_mut(&ns)?.take() {
                    return Some(job);
                }
            }
            state = self.wake.wait(state).ok()?;
        }
    }
    pub fn park(&self, mut job: Job) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                let _ = job.reply.send(Envelope {
                    result: Err(OwnerError::Stopped),
                    _credit: job.credit,
                });
                return;
            }
        };
        if state.stopping {
            drop(state);
            let _ = job.reply.send(Envelope {
                result: Err(OwnerError::Stopped),
                _credit: job.credit,
            });
            return;
        }
        let Some(lane) = state.lanes.get_mut(&job.route.map_or(0, Route::namespace)) else {
            drop(state);
            let _ = job.reply.send(Envelope {
                result: Err(OwnerError::Stopped),
                _credit: job.credit,
            });
            return;
        };
        job.blocked = true;
        lane.queues[job.command.class() as usize].push_front(job);
    }
    pub fn progress(&self, ns: i64, class: ServiceClass, wait: u64, service: u64) {
        if let Ok(mut state) = self.state.lock() {
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
                result: Err(OwnerError::Stopped),
                _credit: job.credit,
            });
        }
        self.wake.notify_all();
    }
}
