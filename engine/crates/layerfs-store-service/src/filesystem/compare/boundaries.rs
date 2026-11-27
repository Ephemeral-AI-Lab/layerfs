//! Disposable bounded boundaries for pinned numbered tree comparisons.
use crate::store::Store;
use layerfs_history::NodeId;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
};
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    pub left: NodeId,
    pub right: NodeId,
    pub prefix: String,
    pub attribution: bool,
    pub hunks: bool,
    pub size: usize,
    pub page: u64,
}
type Entry = (Weak<Store>, Key, Option<String>);
static PAGES: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
/// The last matching path of the preceding page; null means query exhausted.
pub fn find(store: &Arc<Store>, key: &Key) -> Option<Option<String>> {
    let pages = PAGES.lock().unwrap_or_else(|e| e.into_inner());
    pages
        .iter()
        .find(|(owner, k, _)| {
            k == key
                && owner
                    .upgrade()
                    .is_some_and(|owner| Arc::ptr_eq(&owner, store))
        })
        .map(|(_, _, path)| path.clone())
}
/// Cache state is bounded and evictable; a miss changes only lookup work.
pub fn keep(store: &Arc<Store>, key: Key, path: Option<String>) {
    let mut pages = PAGES.lock().unwrap_or_else(|e| e.into_inner());
    pages.retain(|(owner, k, _)| {
        owner.strong_count() > 0
            && !(k == &key
                && owner
                    .upgrade()
                    .is_some_and(|owner| Arc::ptr_eq(&owner, store)))
    });
    pages.push_back((Arc::downgrade(store), key, path));
    while pages.len() > 128 {
        pages.pop_front();
    }
}
