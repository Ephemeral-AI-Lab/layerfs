//! One Workspace lock: connection, open state, lifecycle and counted work.
use super::{backing::Backing, owner::OwnerError};
use crate::{JobWork, OwnerWork, ServiceClass};
use layerfs_overlay::{Overlay, OverlayError};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Instant;

pub struct State {
    pub db: Option<Box<Overlay>>,
    pub work: OwnerWork,
    pub stopping: bool,
    pub failure: Option<Arc<OwnerError>>,
    pub maintenance_error: Option<Arc<OverlayError>>,
    pub cursor: layerfs_overlay::MaintenanceCursor,
    pub initial: Option<Arc<layerfs_overlay::CreationWork>>,
    pub backing: Option<Arc<Backing>>,
    /// Captures waiting for earlier reply attempts, and known installs
    /// waiting for earlier base sources: their frontiers stay finite.
    pub captures_waiting: u32,
    pub installs_waiting: u32,
}
pub struct Shared {
    pub namespace: i64,
    pub route: OnceLock<layerfs_workspace::Route>,
    pub state: Mutex<State>,
    pub changed: Condvar,
}
impl Shared {
    pub fn new(namespace: i64) -> Self {
        Self {
            namespace,
            route: OnceLock::new(),
            state: Mutex::new(State {
                db: None,
                work: OwnerWork::default(),
                stopping: false,
                failure: None,
                maintenance_error: None,
                cursor: layerfs_overlay::MaintenanceCursor::default(),
                initial: None,
                backing: None,
                captures_waiting: 0,
                installs_waiting: 0,
            }),
            changed: Condvar::new(),
        }
    }
}
impl State {
    /// A later mutation waits behind a waiting Capture, and a later source
    /// acquisition behind a waiting known install.
    pub fn behind_frontier(&self, class: ServiceClass) -> bool {
        match class {
            ServiceClass::Mutation => self.captures_waiting != 0,
            ServiceClass::Source => self.installs_waiting != 0,
            _ => false,
        }
    }
    /// Enters or leaves the waiting frontier of one Capture or install. True
    /// when one left: jobs that waited behind it may now run.
    pub fn front(
        &mut self,
        ready: Option<super::owner::Readiness>,
        fronted: &mut bool,
        wanted: bool,
    ) -> bool {
        let count = match ready {
            Some(super::owner::Readiness::Capture) => &mut self.captures_waiting,
            Some(super::owner::Readiness::Install(_)) => &mut self.installs_waiting,
            None => return false,
        };
        match (*fronted, wanted) {
            (false, true) => *count += 1,
            (true, false) => *count -= 1,
            _ => return false,
        }
        *fronted = wanted;
        !wanted
    }
    /// One hold of this connection. A foreground job is counted in its class;
    /// a maintenance step only in the maintenance totals, never in both.
    pub fn record(&mut self, class: ServiceClass, work: &JobWork, maintenance: bool) {
        if maintenance {
            self.work.sql_maintenance.accumulate(work.sql);
            self.work.allocation_maintenance.accumulate(work.allocation);
            self.work.payload_maintenance.accumulate(work.payload);
            self.work.maintenance_jobs = self.work.maintenance_jobs.saturating_add(1);
            self.work.maintenance_ns = self.work.maintenance_ns.saturating_add(work.hold_ns);
        } else {
            self.work.jobs[class as usize] = self.work.jobs[class as usize].saturating_add(1);
            self.work.lock_wait_ns[class as usize] =
                self.work.lock_wait_ns[class as usize].saturating_add(work.lock_wait_ns);
            self.work.hold_ns[class as usize] =
                self.work.hold_ns[class as usize].saturating_add(work.hold_ns);
            self.work.sql_foreground.accumulate(work.sql);
            self.work.allocation_foreground.accumulate(work.allocation);
            self.work.payload_foreground.accumulate(work.payload);
        }
    }
}
pub fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
