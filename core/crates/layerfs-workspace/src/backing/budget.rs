use crate::WorkspaceError;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub(crate) struct Budget {
    limit: usize,
    used: AtomicUsize,
}
pub(crate) struct Charge {
    budget: Arc<Budget>,
    bytes: usize,
}
impl Budget {
    pub fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit,
            used: AtomicUsize::new(0),
        })
    }
    pub fn reserve(self: &Arc<Self>, bytes: usize) -> Result<Charge, WorkspaceError> {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.limit)
            })
            .map_err(|_| WorkspaceError::Capacity)?;
        Ok(Charge {
            budget: self.clone(),
            bytes,
        })
    }
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}
impl Drop for Charge {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
impl Charge {
    /// Adjusts a reservation to the capacity the allocator actually retained.
    pub fn resize(&mut self, bytes: usize) -> Result<(), WorkspaceError> {
        if bytes > self.bytes {
            let mut additional = self.budget.reserve(bytes - self.bytes)?;
            additional.bytes = 0;
        } else {
            self.budget
                .used
                .fetch_sub(self.bytes - bytes, Ordering::AcqRel);
        }
        self.bytes = bytes;
        Ok(())
    }
}
