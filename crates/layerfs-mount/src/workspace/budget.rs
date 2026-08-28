#[derive(Default)]
struct BudgetState {
    current: usize,
    high: usize,
    paused: bool,
    shutdown: bool,
}

pub struct ByteBudget {
    limit: usize,
    state: Mutex<BudgetState>,
    available: Condvar,
}

impl ByteBudget {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            state: Mutex::new(BudgetState::default()),
            available: Condvar::new(),
        }
    }

    pub fn reserve(self: &Arc<Self>, bytes: usize) -> Result<ByteReservation, MountedError> {
        if bytes > self.limit {
            return Err(MountedError::ResourceExhausted);
        }
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        while !state.shutdown && (state.paused || state.current + bytes > self.limit) {
            state = self
                .available
                .wait(state)
                .map_err(|_| MountedError::Indeterminate)?;
        }
        if state.shutdown {
            return Err(MountedError::Busy);
        }
        state.current += bytes;
        state.high = state.high.max(state.current);
        Ok(ByteReservation {
            budget: self.clone(),
            bytes,
        })
    }

    pub fn try_reserve(self: &Arc<Self>, bytes: usize) -> Result<ByteReservation, MountedError> {
        if bytes > self.limit {
            return Err(MountedError::ResourceExhausted);
        }
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        if state.shutdown {
            return Err(MountedError::Busy);
        }
        if state.current + bytes > self.limit {
            return Err(MountedError::ResourceExhausted);
        }
        state.current += bytes;
        state.high = state.high.max(state.current);
        Ok(ByteReservation {
            budget: self.clone(),
            bytes,
        })
    }

    fn observation(&self) -> Result<(usize, usize), MountedError> {
        let state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        Ok((state.current, state.high))
    }

    pub fn pause_and_wait(&self) -> Result<(), MountedError> {
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        if state.shutdown {
            return Err(MountedError::Busy);
        }
        state.paused = true;
        while state.current != 0 {
            state = self
                .available
                .wait(state)
                .map_err(|_| MountedError::Indeterminate)?;
        }
        Ok(())
    }

    pub fn resume(&self) {
        if let Ok(mut state) = self.state.lock() {
            if !state.shutdown {
                state.paused = false;
                self.available.notify_all();
            }
        }
    }

    pub fn close_and_wait(&self) -> Result<(), MountedError> {
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        state.shutdown = true;
        self.available.notify_all();
        while state.current != 0 {
            state = self
                .available
                .wait(state)
                .map_err(|_| MountedError::Indeterminate)?;
        }
        Ok(())
    }

    fn shutdown(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.shutdown = true;
            self.available.notify_all();
        }
    }
}

pub struct ByteReservation {
    budget: Arc<ByteBudget>,
    bytes: usize,
}

impl Drop for ByteReservation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.budget.state.lock() {
            state.current -= self.bytes;
            self.budget.available.notify_all();
        }
    }
}
