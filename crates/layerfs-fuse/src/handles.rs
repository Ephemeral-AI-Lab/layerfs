use crate::NodeId;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone, Copy)]
pub struct OpenHandle {
    pub node: NodeId,
    pub writable: bool,
}

pub struct Handles {
    next: AtomicU64,
    open: Mutex<BTreeMap<u64, OpenHandle>>,
}

impl Default for Handles {
    fn default() -> Self {
        Self {
            next: AtomicU64::new(1),
            open: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Handles {
    pub fn insert(&self, node: NodeId, writable: bool) -> u64 {
        let handle = self.next.fetch_add(1, Ordering::Relaxed);
        self.open
            .lock()
            .expect("FUSE handle table")
            .insert(handle, OpenHandle { node, writable });
        handle
    }

    pub fn get(&self, handle: u64) -> Option<OpenHandle> {
        self.open.lock().ok()?.get(&handle).copied()
    }

    pub fn remove(&self, handle: u64) -> Option<OpenHandle> {
        self.open.lock().ok()?.remove(&handle)
    }
}
