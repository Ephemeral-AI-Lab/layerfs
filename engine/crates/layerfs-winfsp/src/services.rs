//! The bridge between WinFsp callbacks and the mount services: the
//! context one mounted file system carries, and the blocking future
//! runner a synchronous WinFsp callback drives.
use layerfs_mount_ports::ports::{Fence, MountServices, RequestServices};
use layerfs_overlay::KernelMount;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};

/// What one mounted Workspace carries into every callback.
pub struct Context {
    pub services: Arc<dyn MountServices>,
    pub fence: Fence,
    /// This mount kernel token, minted for the in-process WinFsp face.
    pub mount: KernelMount,
}

/// Mints a fresh mount token: engine id 1, a fresh owner, root serial 0.
pub fn mint_mount() -> KernelMount {
    use std::sync::atomic::{AtomicU64, Ordering};
    static OWNER: AtomicU64 = AtomicU64::new(1);
    layerfs_overlay::mint_windows(1, OWNER.fetch_add(1, Ordering::Relaxed), 0)
}

impl Context {
    /// Starts one request services, as each callback does.
    pub fn request(&self) -> Result<Arc<dyn RequestServices>, String> {
        self.services
            .request(&self.fence)
            .map_err(|error| format!("mount services: {error}"))
    }
}

fn noop(_: *const ()) {}
fn clone_raw(_: *const ()) -> RawWaker {
    RawWaker::new(std::ptr::null(), &VTABLE)
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_raw, noop, noop, noop);

/// Drives `future` to completion on the callback thread: each poll runs
/// whatever is ready; Pending yields the thread once (the services the
/// mount face calls complete on their own executor pool, so their Pending
/// arms carry real readiness there).
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut task = TaskContext::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut task) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
