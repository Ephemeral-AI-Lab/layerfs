use crate::WorkspaceError;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub struct Budget {
    limit: usize,
    used: AtomicUsize,
}
pub struct Charge {
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
    #[track_caller]
    pub fn reserve(self: &Arc<Self>, bytes: usize) -> Result<Charge, WorkspaceError> {
        if let Err(used) = self
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.limit)
            })
        {
            if std::env::var_os("LFS_CAPACITY_DIAGNOSTIC").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                let caller = std::panic::Location::caller();
                eprintln!(
                    "LFS_CAPACITY_REFUSAL v=1 domain=memory operation=reserve site={}:{} used={} request={} limit={}",
                    caller.file(), caller.line(), used, bytes, self.limit
                );
            }
            return Err(WorkspaceError::Capacity);
        }
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
    #[track_caller]
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
