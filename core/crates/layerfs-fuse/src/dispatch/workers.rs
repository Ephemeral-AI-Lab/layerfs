//! Fixed daemon-wide worker ownership, startup entry and retained joins.
use super::{
    admission::MountQueue,
    queue::Phase,
    queue::{Lane, Shared, State},
    task,
    types::Failure,
    DispatchConfig, DispatchError, DispatchWork, Shutdown, StartFailure,
};
use layerfs_overlay::NativeMount;
use std::{
    io,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

/// The daemon retains this owner outside request workers and stops it only after
/// every native mount is detached/drained and its queue slot is released.
pub struct Dispatch {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
    shutdown: Option<Shutdown>,
}
impl Dispatch {
    pub fn start(config: DispatchConfig) -> Result<Self, StartFailure> {
        let invalid = || StartFailure {
            reason: DispatchError::InvalidConfig,
            original: None,
            shutdown: Shutdown {
                joins: Vec::new(),
                retained_requests: 0,
                failed: false,
            },
        };
        let workers = config.workers().ok_or_else(invalid)?;
        let mut lanes = Vec::new();
        lanes
            .try_reserve_exact(config.namespaces)
            .map_err(|error| StartFailure {
                reason: DispatchError::Capacity,
                original: Some(io::Error::other(error)),
                shutdown: Shutdown {
                    joins: Vec::new(),
                    retained_requests: 0,
                    failed: false,
                },
            })?;
        lanes.resize_with(config.namespaces, || None);
        let mut threads = Vec::new();
        threads
            .try_reserve_exact(workers)
            .map_err(|error| StartFailure {
                reason: DispatchError::Capacity,
                original: Some(io::Error::other(error)),
                shutdown: Shutdown {
                    joins: Vec::new(),
                    retained_requests: 0,
                    failed: false,
                },
            })?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                lanes,
                next: 0,
                stopping: false,
                failed: false,
                entered: 0,
                live: 0,
                idle: 0,
                observers: 0,
                receiving: 0,
            }),
            changed: Condvar::new(),
            runnable: Condvar::new(),
            workers,
        });
        for number in 0..workers {
            let owned = shared.clone();
            match thread::Builder::new()
                .name(format!("layerfs-fuse-{number}"))
                .spawn(move || worker(owned))
            {
                Ok(handle) => threads.push(handle),
                Err(error) => {
                    stop_workers(&shared);
                    let shutdown = join_workers(&shared, &mut threads);
                    return Err(StartFailure {
                        reason: DispatchError::Failed,
                        original: Some(error),
                        shutdown,
                    });
                }
            }
        }
        let mut state = shared.lock();
        while state.entered != workers && !state.failed {
            state = shared.wait(state);
        }
        let failed = state.failed;
        drop(state);
        if failed {
            stop_workers(&shared);
            let shutdown = join_workers(&shared, &mut threads);
            return Err(StartFailure {
                reason: DispatchError::Failed,
                original: None,
                shutdown,
            });
        }
        Ok(Self {
            shared,
            threads,
            shutdown: None,
        })
    }
    pub fn register(&self, mount: NativeMount) -> Result<MountQueue, DispatchError> {
        let mut state = self.shared.lock();
        if state.failed {
            return Err(DispatchError::Failed);
        }
        if state.stopping {
            return Err(DispatchError::Stopped);
        }
        if state
            .lanes
            .iter()
            .flatten()
            .any(|lane| lane.mount.route() == mount.route())
        {
            return Err(DispatchError::DuplicateMount);
        }
        let index = state
            .lanes
            .iter()
            .position(Option::is_none)
            .ok_or(DispatchError::Capacity)?;
        state.lanes[index] = Some(Lane::new(mount));
        let token = state.lanes[index].as_ref().unwrap().token.clone();
        Ok(MountQueue {
            shared: self.shared.clone(),
            index,
            mount,
            token,
        })
    }
    pub fn work(&self) -> DispatchWork {
        let state = self.shared.lock();
        let mut work = DispatchWork {
            configured_workers: self.shared.workers,
            entered_workers: state.entered,
            live_workers: state.live,
            stopping: state.stopping,
            failed: state.failed,
            ..DispatchWork::default()
        };
        for lane in state.lanes.iter().flatten() {
            let current = lane.observe();
            work.mounts += 1;
            work.queued += current.queued;
            work.running += current.running;
            work.parked += current.parked;
            work.retained += current.retained;
        }
        work
    }
    /// A Busy refusal has no effect. Repeated observation returns the original
    /// stored join outcomes; it never turns a consumed failed join into success.
    pub fn stop(&mut self) -> Result<&Shutdown, DispatchError> {
        if self.shutdown.is_none() {
            if self.shared.lock().lanes.iter().any(Option::is_some) {
                return Err(DispatchError::Busy);
            }
            stop_workers(&self.shared);
            self.shutdown = Some(join_workers(&self.shared, &mut self.threads));
        }
        Ok(self.shutdown.as_ref().unwrap())
    }
    /// Fence an already failed pool and retain every original join outcome.
    /// This does not dispose retained requests or establish connection drain.
    pub fn join_failed(&mut self) -> Result<&Shutdown, DispatchError> {
        if self.shutdown.is_none() {
            if !self.shared.lock().failed {
                return Err(DispatchError::Busy);
            }
            stop_workers(&self.shared);
            self.shutdown = Some(join_workers(&self.shared, &mut self.threads));
        }
        Ok(self.shutdown.as_ref().unwrap())
    }
}
impl Drop for Dispatch {
    fn drop(&mut self) {
        if self.shutdown.is_none() {
            stop_workers(&self.shared);
            self.shutdown = Some(join_workers(&self.shared, &mut self.threads));
        }
        // MountQueue owners retain stopped state and original request custody.
        // Drop is no native detach, filesystem drain or successful stop receipt.
    }
}
fn stop_workers(shared: &Shared) {
    let mut state = shared.lock();
    state.stopping = true;
    for lane in state.lanes.iter_mut().flatten() {
        lane.work.terminal = true;
    }
    shared.wake_all();
}
fn join_workers(shared: &Shared, threads: &mut Vec<JoinHandle<()>>) -> Shutdown {
    let joins = threads.drain(..).map(JoinHandle::join).collect();
    let mut state = shared.lock();
    // A first step still running on its receive thread ends itself; the
    // requests left after it are the retained ones.
    while state.receiving != 0 {
        state = shared.wait(state);
    }
    let reason = if state.failed {
        DispatchError::Failed
    } else {
        DispatchError::Stopped
    };
    for lane in state.lanes.iter_mut().flatten() {
        lane.ready.clear();
        for entry in lane.tasks.iter_mut().flatten() {
            if let Some(task) = &entry.task {
                let mut failure = task
                    .failure
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                if failure.is_none() {
                    *failure = Some(Failure::Request(Box::new(reason)));
                }
                entry.phase = Phase::Retained;
            }
        }
    }
    Shutdown {
        joins,
        retained_requests: state
            .lanes
            .iter()
            .flatten()
            .map(|lane| lane.work.admitted)
            .sum(),
        failed: state.failed,
    }
}
struct Exit(Arc<Shared>);
impl Drop for Exit {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.live -= 1;
        if !state.stopping {
            state.failed = true;
            state.stopping = true;
            for lane in state.lanes.iter_mut().flatten() {
                lane.work.terminal = true;
            }
        }
        self.0.wake_all();
    }
}
fn worker(shared: Arc<Shared>) {
    {
        let mut state = shared.lock();
        state.entered += 1;
        state.live += 1;
        shared.announce(&state);
    }
    let _exit = Exit(shared.clone());
    loop {
        let mut state = shared.lock();
        let task = loop {
            if state.stopping {
                return;
            }
            if let Some(task) = state.take() {
                break task;
            }
            state = shared.wait_runnable(state);
        };
        drop(state);
        task::advance(task, &shared, false);
    }
}
