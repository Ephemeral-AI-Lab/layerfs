//! Undo only touched open-state entries; never copy the resident map.
use crate::{OverlayError, OverlayResult};
use std::collections::BTreeMap;
use std::ops::RangeBounds;

pub(crate) struct JournalMap<K, V> {
    pub rows: BTreeMap<K, V>,
    undo: Vec<(K, Option<V>)>,
}
impl<K, V> Default for JournalMap<K, V> {
    fn default() -> Self {
        Self {
            rows: BTreeMap::new(),
            undo: Vec::new(),
        }
    }
}
impl<K: Ord + Clone, V> JournalMap<K, V> {
    pub fn put(&mut self, key: K, value: V) {
        let before = self.rows.insert(key.clone(), value);
        self.undo.push((key, before));
    }
    pub fn remove(&mut self, key: K) {
        if let Some(before) = self.rows.remove(&key) {
            self.undo.push((key, Some(before)));
        }
    }
    pub fn commit(&mut self) {
        self.undo.clear();
    }
    pub fn rollback(&mut self) {
        for (key, before) in self.undo.drain(..).rev() {
            match before {
                Some(value) => {
                    self.rows.insert(key, value);
                }
                None => {
                    self.rows.remove(&key);
                }
            }
        }
    }
}
/// Values held by a minted owner, each found again by the exact caller
/// request that created it; both indexes journal together.
pub(crate) struct OwnerTable<R, V> {
    owners: JournalMap<u64, (V, R)>,
    requests: JournalMap<R, u64>,
}
impl<R, V> Default for OwnerTable<R, V> {
    fn default() -> Self {
        Self {
            owners: JournalMap::default(),
            requests: JournalMap::default(),
        }
    }
}
impl<R: Ord + Copy, V: Copy> OwnerTable<R, V> {
    pub fn insert(
        &mut self,
        owner: u64,
        request: R,
        value: V,
        duplicate: &'static str,
    ) -> OverlayResult<()> {
        if self.requests.rows.contains_key(&request) {
            return Err(OverlayError::Invalid(duplicate));
        }
        self.owners.put(owner, (value, request));
        self.requests.put(request, owner);
        Ok(())
    }
    pub fn get(&self, owner: u64) -> Option<(V, R)> {
        self.owners.rows.get(&owner).copied()
    }
    pub fn retained(&self, request: R) -> Option<V> {
        let owner = self.requests.rows.get(&request)?;
        self.owners.rows.get(owner).map(|(value, _)| *value)
    }
    pub fn any_request(&self, range: impl RangeBounds<R>) -> bool {
        self.requests.rows.range(range).next().is_some()
    }
    pub fn update(&mut self, owner: u64, value: V) -> OverlayResult<()> {
        let (_, request) = self.get(owner).ok_or(OverlayError::Stale)?;
        self.owners.put(owner, (value, request));
        Ok(())
    }
    /// Removes the owner's value if `exact` accepts it; otherwise Stale.
    pub fn remove(&mut self, owner: u64, exact: impl FnOnce(&V) -> bool) -> OverlayResult<()> {
        let (held, request) = self.get(owner).ok_or(OverlayError::Stale)?;
        if !exact(&held) {
            return Err(OverlayError::Stale);
        }
        self.owners.remove(owner);
        self.requests.remove(request);
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.owners.rows.len()
    }
    pub fn commit(&mut self) {
        self.owners.commit();
        self.requests.commit();
    }
    pub fn rollback(&mut self) {
        self.owners.rollback();
        self.requests.rollback();
    }
}
