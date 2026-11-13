//! Retained child custody, one implementation for every caller that stops a
//! helper with a deadline: the daemon's NFS mount and unmount helpers and the
//! macOS driver's `umount -f` (layerfs-project#136). A child that survived its
//! KILL and the post-KILL grace window is never waited on synchronously and is
//! never signalled twice. One bounded background thread polls it instead and
//! records the exit it observes.
use std::{
    fmt,
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// Between two looks at the retained queue.
const PAUSE: Duration = Duration::from_millis(20);
/// Observed exits kept for diagnostics; the oldest go first.
const RECORDED: usize = 64;

/// Children a caller could not observe exiting, and the exits seen since.
/// `retain`, `reap`, `exits` and `record` are the whole surface: nothing here
/// signals a child, and no call waits for one.
#[derive(Clone, Debug, Default)]
pub struct Retained(Arc<Inner>);

#[derive(Debug, Default)]
struct Inner {
    children: Mutex<Vec<Child>>,
    exits: Mutex<Vec<(u32, String)>>,
    reaping: AtomicBool,
}

impl Retained {
    /// Keeps `child` and makes sure one background reaper is polling for it.
    pub fn retain(&self, child: Child) {
        self.reap();
        self.0
            .children
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(child);
        if self.0.reaping.swap(true, Ordering::AcqRel) {
            return;
        }
        let queue = self.0.clone();
        if std::thread::Builder::new()
            .name("layerfs-child-reap".into())
            .spawn(move || loop {
                if queue.reap() {
                    queue.reaping.store(false, Ordering::Release);
                    if queue.reap() || queue.reaping.swap(true, Ordering::AcqRel) {
                        break;
                    }
                }
                std::thread::sleep(PAUSE);
            })
            .is_err()
        {
            self.0.reaping.store(false, Ordering::Release);
        }
    }

    /// Reaps every child that has exited; true when none remain.
    pub fn reap(&self) -> bool {
        self.0.reap()
    }

    /// The exits observed so far, oldest first.
    pub fn exits(&self) -> Vec<(u32, String)> {
        self.reap();
        self.0
            .exits
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// Records an exit the caller observed itself with its own `try_wait`.
    pub fn record(&self, child: u32, status: impl fmt::Display) {
        let mut exits = self
            .0
            .exits
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        exits.push((child, status.to_string()));
        let excess = exits.len().saturating_sub(RECORDED);
        exits.drain(..excess);
    }
}

impl Inner {
    fn reap(&self) -> bool {
        let mut children = self
            .children
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut exits = self.exits.lock().unwrap_or_else(|error| error.into_inner());
        children.retain_mut(|child| match child.try_wait() {
            Ok(Some(status)) => {
                exits.push((child.id(), status.to_string()));
                false
            }
            Ok(None) => true,
            Err(error) => {
                exits.push((child.id(), format!("wait error: {error}")));
                false
            }
        });
        let excess = exits.len().saturating_sub(RECORDED);
        exits.drain(..excess);
        children.is_empty()
    }
}
