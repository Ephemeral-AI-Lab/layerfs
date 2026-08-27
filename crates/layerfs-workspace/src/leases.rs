use crate::{Result, WorkspaceError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseKind {
    Callback,
    Process,
    Writer,
    Descriptor,
    WritableMapping,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeObservation {
    pub callbacks: u64,
    pub processes: u64,
    pub writers: u64,
    pub descriptors: u64,
    pub writable_mappings: u64,
}

impl RuntimeObservation {
    fn is_quiescent(self) -> bool {
        self == Self::default()
    }

    fn increment(&mut self, kind: LeaseKind) -> Option<()> {
        let value = match kind {
            LeaseKind::Callback => &mut self.callbacks,
            LeaseKind::Process => &mut self.processes,
            LeaseKind::Writer => &mut self.writers,
            LeaseKind::Descriptor => &mut self.descriptors,
            LeaseKind::WritableMapping => &mut self.writable_mappings,
        };
        *value = value.checked_add(1)?;
        Some(())
    }

    fn decrement(&mut self, kind: LeaseKind) {
        let value = match kind {
            LeaseKind::Callback => &mut self.callbacks,
            LeaseKind::Process => &mut self.processes,
            LeaseKind::Writer => &mut self.writers,
            LeaseKind::Descriptor => &mut self.descriptors,
            LeaseKind::WritableMapping => &mut self.writable_mappings,
        };
        *value = value.saturating_sub(1);
    }
}

struct LeaseState {
    accepting: bool,
    observation: RuntimeObservation,
}

struct SharedLeases {
    state: Mutex<LeaseState>,
    changed: Condvar,
}

#[derive(Clone)]
pub struct RuntimeLeases(Arc<SharedLeases>);

impl Default for RuntimeLeases {
    fn default() -> Self {
        Self(Arc::new(SharedLeases {
            state: Mutex::new(LeaseState {
                accepting: true,
                observation: RuntimeObservation::default(),
            }),
            changed: Condvar::new(),
        }))
    }
}

impl RuntimeLeases {
    pub fn acquire(&self, kind: LeaseKind) -> Result<LeaseGuard> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| WorkspaceError::InvalidState)?;
        if !state.accepting {
            return Err(WorkspaceError::Busy);
        }
        state
            .observation
            .increment(kind)
            .ok_or(WorkspaceError::ResourceExhausted)?;
        Ok(LeaseGuard {
            leases: self.clone(),
            kind,
            active: true,
        })
    }

    pub fn observation(&self) -> Result<RuntimeObservation> {
        self.0
            .state
            .lock()
            .map(|state| state.observation)
            .map_err(|_| WorkspaceError::InvalidState)
    }

    pub(crate) fn close_and_wait(&self, timeout: Duration) -> Result<RuntimeObservation> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(WorkspaceError::ResourceExhausted)?;
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| WorkspaceError::InvalidState)?;
        state.accepting = false;
        while !state.observation.is_quiescent() {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(WorkspaceError::Timeout)?;
            let (next, wait) = self
                .0
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| WorkspaceError::InvalidState)?;
            state = next;
            if wait.timed_out() && !state.observation.is_quiescent() {
                return Err(WorkspaceError::Timeout);
            }
        }
        Ok(state.observation)
    }

    fn release(&self, kind: LeaseKind) {
        if let Ok(mut state) = self.0.state.lock() {
            state.observation.decrement(kind);
            self.0.changed.notify_all();
        }
    }
}

pub struct LeaseGuard {
    leases: RuntimeLeases,
    kind: LeaseKind,
    active: bool,
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        if self.active {
            self.leases.release(self.kind);
            self.active = false;
        }
    }
}
