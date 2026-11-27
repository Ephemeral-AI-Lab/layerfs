//! Weak-Store-qualified, bounded boundaries for immutable namespace result pages.
use crate::store::Store;
use layerfs_history::NodeId;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
};
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    pub node: NodeId,
    pub mode: &'static str,
    pub path: String,
    pub pattern: String,
    pub size: usize,
    pub page: u64,
}
#[derive(Clone)]
pub struct Boundary {
    pub exhausted: bool,
    pub path: String,
    pub line: u64,
    pub skipped: i64,
}
type Entry = (Weak<Store>, Key, Option<Boundary>);
static RECENT: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
/// A null boundary proves exhaustion; absent cache state requires bounded replay.
pub fn find(store: &Arc<Store>, key: &Key) -> Option<Option<Boundary>> {
    let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    recent.retain(|(owner, _, _)| owner.strong_count() > 0);
    recent
        .iter()
        .find(|(owner, k, _)| k == key && owner.ptr_eq(&Arc::downgrade(store)))
        .map(|(_, _, boundary)| boundary.clone())
}
/// Keep at most 128 page boundaries. Cache eviction affects cost, never answers.
pub fn keep(store: &Arc<Store>, key: Key, boundary: Option<Boundary>) {
    let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    recent.retain(|(owner, k, _)| {
        owner.strong_count() > 0 && !(k == &key && owner.ptr_eq(&Arc::downgrade(store)))
    });
    recent.push_back((Arc::downgrade(store), key, boundary));
    while recent.len() > 128 {
        recent.pop_front();
    }
}
