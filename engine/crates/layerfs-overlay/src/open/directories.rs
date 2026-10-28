//! Directory reply positions with per-job undo and disposal outside service.
use super::OpenState;
use crate::{KernelDirectory, Overlay, OverlayError, OverlayResult};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy)]
pub(crate) struct DirectoryState {
    pub directory: KernelDirectory,
    pub floor: u64,
}
pub(crate) struct CookiePage {
    pub after: Vec<u8>,
    pub names: Vec<u8>,
}
#[derive(Default)]
pub(crate) struct CookieStore {
    pub offsets: BTreeMap<u64, Arc<CookiePage>>,
    pub after: BTreeMap<(Vec<u8>, u64), ()>,
}
pub(crate) enum CookieUndo {
    Open(u64),
    Close(u64, CookieStore),
    Page(u64, u64, Option<Arc<CookiePage>>),
}
/// Detached directory reply allocations. The caller drops these after releasing
/// its connection turn; destroying a large listing holds no Workspace lock.
#[derive(Default)]
pub struct OpenDisposals {
    stores: Vec<CookieStore>,
}
impl CookieStore {
    fn put(&mut self, first: u64, page: Arc<CookiePage>) -> Option<Arc<CookiePage>> {
        let old = self.offsets.insert(first, page.clone());
        if let Some(old) = &old {
            self.after.remove(&(old.after.clone(), first));
        }
        self.after.insert((page.after.clone(), first), ());
        old
    }
    fn remove(&mut self, first: u64) -> Option<Arc<CookiePage>> {
        let old = self.offsets.remove(&first)?;
        self.after.remove(&(old.after.clone(), first));
        Some(old)
    }
}
impl OpenState {
    pub fn open_cookies(&mut self, key: u64) -> OverlayResult<()> {
        if self.cookies.contains_key(&key) {
            return Err(OverlayError::Stale);
        }
        self.cookies.insert(key, CookieStore::default());
        self.cookie_undo.push(CookieUndo::Open(key));
        Ok(())
    }
    pub fn close_cookies(&mut self, key: u64) -> OverlayResult<()> {
        let store = self.cookies.remove(&key).ok_or(OverlayError::Stale)?;
        self.cookie_undo.push(CookieUndo::Close(key, store));
        Ok(())
    }
    pub fn put_cookie(&mut self, key: u64, first: u64, page: CookiePage) -> OverlayResult<()> {
        let store = self.cookies.get_mut(&key).ok_or(OverlayError::Stale)?;
        if store.offsets.contains_key(&first) {
            return Err(OverlayError::Invalid("duplicate directory cookie"));
        }
        let before = store.put(first, Arc::new(page));
        self.cookie_undo.push(CookieUndo::Page(key, first, before));
        Ok(())
    }
    pub fn retire_cookies(&mut self, key: u64, floor: u64, maximum: usize) -> OverlayResult<()> {
        let store = self.cookies.get_mut(&key).ok_or(OverlayError::Stale)?;
        let old = store
            .offsets
            .range(..floor)
            .take(maximum)
            .map(|(first, _)| *first)
            .collect::<Vec<_>>();
        for first in old {
            let before = self
                .cookies
                .get_mut(&key)
                .ok_or(OverlayError::Stale)?
                .remove(first);
            self.cookie_undo.push(CookieUndo::Page(key, first, before));
        }
        Ok(())
    }
    pub fn commit_cookies(&mut self) {
        for undo in self.cookie_undo.drain(..) {
            if let CookieUndo::Close(_, store) = undo {
                self.disposals.stores.push(store);
            }
        }
    }
    pub fn rollback_cookies(&mut self) {
        for undo in self.cookie_undo.drain(..).rev() {
            match undo {
                CookieUndo::Open(key) => {
                    self.cookies.remove(&key);
                }
                CookieUndo::Close(key, store) => {
                    self.cookies.insert(key, store);
                }
                CookieUndo::Page(key, first, old) => {
                    let store = self
                        .cookies
                        .get_mut(&key)
                        .expect("cookie undo owns directory");
                    match old {
                        Some(page) => {
                            store.put(first, page);
                        }
                        None => {
                            store.remove(first);
                        }
                    }
                }
            }
        }
    }
}
impl Overlay {
    /// Transfers only already-detached reply allocations. It performs no SQL,
    /// mutation, cleanup job, replay or filesystem operation.
    pub fn take_open_disposals(&self) -> OpenDisposals {
        std::mem::take(&mut self.open.borrow_mut().disposals)
    }
}
