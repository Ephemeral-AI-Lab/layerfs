//! Prototype (lanes2 probe, #684; not a product contract): whole pack bodies
//! the read lanes of one mounted Workspace share.
//!
//! Each lane is its own reader with its own pack cache, so a pack whose
//! records fall in two lanes' demands (the edge of a read-ahead part, or the
//! edge of the next window) is read from the Store twice. A lane that reads
//! a demand's first or last pack whole leaves the body here, already checked
//! against its descriptor's SHA-256 key and frame; a lane that needs it later
//! copies it from here instead of reading it again. Bodies never change once
//! published, so a kept body stays valid. Bounded by [`SHARED_BYTES`],
//! oldest first out.
use crate::location::PackInfo;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
};

/// Most body bytes kept: 4 MiB.
const SHARED_BYTES: usize = 4 * 1024 * 1024;

/// Prototype: see the module documentation.
#[derive(Default)]
pub struct SharedPacks {
    bodies: Mutex<(VecDeque<(PackInfo, Arc<Vec<u8>>)>, usize)>,
    /// Read an edge pack whole even when its groups alone would be read.
    edge_whole: bool,
    hits: AtomicU64,
    kept: AtomicU64,
}
impl SharedPacks {
    /// An empty map; `edge_whole` reads each demand's edge packs whole.
    pub fn new(edge_whole: bool) -> Self {
        Self {
            edge_whole,
            ..Self::default()
        }
    }
    /// Whether a demand's edge packs are read whole.
    pub(crate) fn edge_whole(&self) -> bool {
        self.edge_whole
    }
    /// Lanes that copied a body from here, and bodies kept here.
    pub fn work(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.kept.load(Ordering::Relaxed),
        )
    }
    pub(crate) fn get(&self, id: i64) -> Option<(PackInfo, Arc<Vec<u8>>)> {
        let bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        let found = bodies
            .0
            .iter()
            .find(|(info, _)| info.pack_id == id)
            .cloned();
        if found.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        }
        found
    }
    pub(crate) fn keep(&self, info: PackInfo, body: &[u8]) {
        if body.len() > SHARED_BYTES {
            return;
        }
        let mut bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        if bodies
            .0
            .iter()
            .any(|(kept, _)| kept.pack_id == info.pack_id)
        {
            return;
        }
        while bodies.1 + body.len() > SHARED_BYTES {
            let Some((_, old)) = bodies.0.pop_front() else {
                break;
            };
            bodies.1 -= old.len();
        }
        bodies.1 += body.len();
        bodies.0.push_back((info, Arc::new(body.to_vec())));
        self.kept.fetch_add(1, Ordering::Relaxed);
    }
}
