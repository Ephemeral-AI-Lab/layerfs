//! Retained child custody, one implementation for every caller that stops a
//! helper with a deadline: the daemon's NFS mount and unmount helpers and the
//! macOS driver's `umount -f` (layerfs-project#136). A child that survived its
//! KILL and the post-KILL grace window is never waited on synchronously and is
//! never signalled twice. One background thread polls it instead and records
//! the exit it observes.
//!
//! The bound is one reaper thread and [`RECORDED`] observed exits, not time: a
//! child whose exit never arrives keeps the queue non-empty and the one thread
//! polling every [`PAUSE`] for as long as its custody lasts. That is the point
//! of keeping it; a deadline would drop the only handle that can reap it.
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
/// Consecutive failed waits after which a child is given up on (~0.5 s at
/// [`PAUSE`]). A wait that keeps failing cannot reap the child, so holding its
/// handle only polls a broken one; the failure is recorded instead.
const ERROR_TRIES: u8 = 25;

/// Children a caller could not observe exiting, and the exits seen since.
/// `retain`, `reap`, `exits` and `record` are the whole surface: nothing here
/// signals a child, and no call waits for one.
#[derive(Clone, Debug, Default)]
pub struct Retained(Arc<Inner>);

#[derive(Debug, Default)]
struct Inner {
    children: Mutex<Vec<Kept>>,
    exits: Mutex<Vec<(u32, String)>>,
    reaping: AtomicBool,
}

/// One retained child and, when its last wait failed, that failure.
#[derive(Debug)]
struct Kept {
    child: Child,
    error: Option<String>,
    failures: u8,
}

impl Retained {
    /// Keeps `child` and makes sure one background reaper is polling for it.
    pub fn retain(&self, child: Child) {
        self.reap();
        self.0
            .children
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Kept {
                child,
                error: None,
                failures: 0,
            });
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
        children.retain_mut(|kept| match kept.child.try_wait() {
            Ok(Some(status)) => {
                exits.push((kept.child.id(), status.to_string()));
                false
            }
            Ok(None) => {
                kept.error = None;
                kept.failures = 0;
                true
            }
            // A failed wait is not an exit. Keep the child so its exit is
            // still observed, record the failure once (changed message or the
            // final give-up), and stop polling it after `ERROR_TRIES`.
            Err(error) => {
                let message = error.to_string();
                if kept.error.as_deref() != Some(message.as_str()) {
                    kept.error = Some(message.clone());
                    kept.failures = 0;
                }
                kept.failures = kept.failures.saturating_add(1);
                if kept.failures < ERROR_TRIES {
                    return true;
                }
                exits.push((
                    kept.child.id(),
                    format!("exit unobservable after {ERROR_TRIES} failed waits: {message}"),
                ));
                false
            }
        });
        let excess = exits.len().saturating_sub(RECORDED);
        exits.drain(..excess);
        children.is_empty()
    }
}
