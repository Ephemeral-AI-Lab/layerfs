use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use super::{VfsError, VfsResult};

#[derive(Default)]
pub(super) struct LeaseState {
    writers: usize,
    capturing: bool,
    terminal: bool,
}
pub(super) type SharedLeaseState = Arc<Mutex<LeaseState>>;
type WriterStateRegistry = Mutex<HashMap<Vec<u8>, Weak<Mutex<LeaseState>>>>;

static WRITER_STATES: OnceLock<WriterStateRegistry> = OnceLock::new();

pub(super) fn shared_writers(identity: &[u8]) -> VfsResult<SharedLeaseState> {
    let states = WRITER_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut states = states.lock().map_err(|_| VfsError::InvalidState)?;
    states.retain(|_, state| state.strong_count() != 0);
    if let Some(state) = states.get(identity).and_then(Weak::upgrade) {
        return Ok(state);
    }
    let state = Arc::new(Mutex::new(LeaseState::default()));
    states.insert(identity.to_vec(), Arc::downgrade(&state));
    Ok(state)
}

pub(super) struct CaptureLease {
    state: SharedLeaseState,
    active: bool,
}

impl CaptureLease {
    pub(super) fn begin(state: SharedLeaseState) -> VfsResult<Self> {
        {
            let mut value = state.lock().map_err(|_| VfsError::InvalidState)?;
            if value.terminal {
                return Err(VfsError::InvalidState);
            }
            if value.capturing || value.writers != 0 {
                return Err(VfsError::WorkspaceBusy);
            }
            value.capturing = true;
        }
        Ok(Self {
            state,
            active: true,
        })
    }
    pub(super) fn finish(&mut self) -> VfsResult<()> {
        let mut state = self.state.lock().map_err(|_| VfsError::InvalidState)?;
        state.capturing = false;
        state.terminal = true;
        self.active = false;
        Ok(())
    }
}

impl Drop for CaptureLease {
    fn drop(&mut self) {
        if self.active {
            if let Ok(mut state) = self.state.lock() {
                state.capturing = false;
            }
        }
    }
}

pub struct WriterLease(SharedLeaseState);
impl WriterLease {
    pub(super) fn begin(state: SharedLeaseState) -> VfsResult<Self> {
        {
            let mut value = state.lock().map_err(|_| VfsError::InvalidState)?;
            if value.capturing || value.terminal {
                return Err(VfsError::WorkspaceBusy);
            }
            value.writers = value.writers.checked_add(1).ok_or(VfsError::InvalidState)?;
        }
        Ok(Self(state))
    }
}
impl Drop for WriterLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.writers = state.writers.saturating_sub(1);
        }
    }
}
