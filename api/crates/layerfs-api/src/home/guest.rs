//! A worker's root is not a home (H §7.7, O33). The worker runs host code
//! with `home` set to its root (a machine's data disk), and that code asks
//! for the home's id (container names carry it). The root is never
//! converted, created or given an id of its own: the id is the caller's,
//! sent with each request.
//!
//! On a caller's thread, the id is the one of the home the thread last
//! opened (every call opens its home first); the worker client sends it.
//! On a worker's thread, it is the one the request being served carried.
use super::layout::{HomeFile, LAYOUT_VERSION};
use crate::host::outcome::refused;
use layerfs_bridge::control::SdkRefusal;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The roots this process's workers serve.
static ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

thread_local! {
    static ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Marks `root` as a worker's root: opening it never writes it.
pub(crate) fn serve_root(root: &Path) {
    let mut roots = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    if !roots.iter().any(|r| r == root) {
        roots.push(root.to_owned());
    }
}

/// Whether `path` is a root a worker of this process serves.
pub(super) fn is_root(path: &Path) -> bool {
    let roots = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    roots.iter().any(|r| r == path)
}

/// Notes `id` as the home this thread opened.
pub(super) fn opened(id: &str) {
    ID.with(|slot| *slot.borrow_mut() = Some(id.to_owned()));
}

/// The id of the home this thread works for, if it has one.
pub(crate) fn current_id() -> Option<String> {
    ID.with(|slot| slot.borrow().clone())
}

/// A worker root opened as a home: the caller's id, or refused when the
/// request carried none. Nothing is read or written.
pub(super) fn file(root: &Path) -> Result<HomeFile, SdkRefusal> {
    match current_id() {
        Some(id) if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) => Ok(HomeFile {
            layout: LAYOUT_VERSION,
            id,
        }),
        _ => Err(refused(
            "InvalidInput",
            format!(
                "{} is a worker's root, not a home; the caller sent no home id",
                root.display()
            ),
        )),
    }
}

/// The home id a request carried, for as long as it is served on this
/// thread.
pub(crate) struct Sent(Option<String>);

/// Serves one request with `id` (the caller's home id) as this thread's.
pub(crate) fn sent(id: Option<&str>) -> Sent {
    Sent(ID.with(|slot| slot.replace(id.map(str::to_owned))))
}

impl Drop for Sent {
    fn drop(&mut self) {
        let before = self.0.take();
        ID.with(|slot| *slot.borrow_mut() = before);
    }
}
