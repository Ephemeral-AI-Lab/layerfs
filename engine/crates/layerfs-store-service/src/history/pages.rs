//! Bounded, disposable page boundaries for immutable saved-history queries.
//! Eviction changes query cost only; it never changes saved results.
use crate::store::Store;
use layerfs_history::NodeId;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
};
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    /// A live Store instance; saved IDs additionally qualify all boundaries.
    pub store: usize,
    pub start: NodeId,
    pub branch: String,
    pub size: usize,
    pub page: u64,
}
static PAGES: Mutex<VecDeque<(Key, Weak<Store>, Option<NodeId>)>> = Mutex::new(VecDeque::new());
/// A saved page's first node, if an earlier page already established it.
pub fn find(store: &Arc<Store>, key: &Key) -> Option<Option<NodeId>> {
    let pages = PAGES.lock().unwrap_or_else(|e| e.into_inner());
    pages
        .iter()
        .find(|(k, owner, _)| {
            k == key
                && owner
                    .upgrade()
                    .is_some_and(|owner| Arc::ptr_eq(&owner, store))
        })
        .map(|(_, _, n)| *n)
}
/// Retain at most 128 immutable boundaries; no public cursor or durable state.
pub fn keep(store: &Arc<Store>, key: Key, node: Option<NodeId>) {
    let mut pages = PAGES.lock().unwrap_or_else(|e| e.into_inner());
    pages.retain(|(k, owner, _)| {
        owner.strong_count() > 0
            && !(k == &key
                && owner
                    .upgrade()
                    .is_some_and(|owner| Arc::ptr_eq(&owner, store)))
    });
    pages.push_back((key, Arc::downgrade(store), node));
    while pages.len() > 128 {
        pages.pop_front();
    }
}
