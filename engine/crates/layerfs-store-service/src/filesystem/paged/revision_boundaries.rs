//! Bounded immutable revision-page boundaries; failed queries never populate it.
use crate::store::Store;
use layerfs_history::NodeId;
use std::sync::{Arc, Mutex, Weak};

#[derive(Clone, Copy)]
pub struct Boundary {
    pub next: Option<NodeId>,
    pub candidate: u64,
}

pub struct Key<'a> {
    pub start: NodeId,
    pub path: &'a str,
    pub traversal: &'a str,
    pub branch: &'a str,
    pub size: usize,
}

struct Query {
    store: Weak<Store>,
    start: NodeId,
    path: String,
    traversal: String,
    branch: String,
    size: usize,
    boundaries: Vec<(u64, Boundary)>,
}

static RECENT: Mutex<Vec<Query>> = Mutex::new(Vec::new());

fn agrees(q: &Query, store: &Arc<Store>, key: &Key<'_>) -> bool {
    q.store.ptr_eq(&Arc::downgrade(store))
        && q.start == key.start
        && q.path == key.path
        && q.traversal == key.traversal
        && q.branch == key.branch
        && q.size == key.size
}

/// A Weak Store lifetime guard prevents address reuse and keeps no Store alive.
pub fn find(store: &Arc<Store>, key: &Key<'_>, skip: u64) -> Option<Boundary> {
    let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    recent.retain(|q| q.store.strong_count() > 0);
    let index = recent.iter().position(|q| agrees(q, store, key))?;
    let q = recent.remove(index);
    let found = q
        .boundaries
        .iter()
        .find(|(n, _)| *n == skip)
        .map(|(_, b)| *b);
    recent.push(q);
    found
}

/// At most four queries with 128 boundaries each. Eviction changes cost, not facts.
pub fn keep(store: &Arc<Store>, key: &Key<'_>, skip: u64, boundary: Boundary) {
    let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    recent.retain(|q| q.store.strong_count() > 0);
    let mut q = match recent.iter().position(|q| agrees(q, store, key)) {
        Some(index) => recent.remove(index),
        None => Query {
            store: Arc::downgrade(store),
            start: key.start,
            path: key.path.to_owned(),
            traversal: key.traversal.to_owned(),
            branch: key.branch.to_owned(),
            size: key.size,
            boundaries: Vec::new(),
        },
    };
    q.boundaries.retain(|(n, _)| *n != skip);
    if q.boundaries.len() == 128 {
        q.boundaries.remove(0);
    }
    q.boundaries.push((skip, boundary));
    if recent.len() == 4 {
        recent.remove(0);
    }
    recent.push(q);
}
