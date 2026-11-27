//! How an admitted request ends itself instead of being retained with its
//! mount fenced: after its mount's stopped fence refused it before an
//! attempt, after a base demand made for it alone failed, or after a read or
//! directory visit found that the serial it names was dropped by an install.
//!
//! Forced teardown stops the fence after its abort write. A request waiting
//! for owner admission or a Store reader then fails with `Fenced` instead of
//! attempting anything; its single reply attempt answers [`STOPPED`] and is
//! counted on the fence. A request whose canonical base demand failed carries
//! `BaseDemandFailed`; its single reply attempt answers `EIO`, its cause is
//! already in the mount's bounded record, and later requests are served. A
//! read or directory request whose serial an install dropped while the kernel
//! held it (`stale()`: a merge rebuilt that directory or file under a fresh
//! serial) answers `ESTALE` and is not recorded; the mount serves on.
//!
//! Either way the request then releases what it still owns through the
//! disposal ports, which the fence never refuses, and makes no further demand.
//! A request whose release fails is retained with what remains, as any other
//! failed release. Every other failure is retained exactly as before: an
//! owner job's failure, an uncertain outcome, a failed reply-ticket step. A
//! job already submitted is never ended here: its request receives the
//! original result first.
use crate::{request::coherence, RequestDisposition};
use fuser::Errno;
use layerfs_vfs::{
    ports::mount::Fence,
    requests::{DirectoryFailure, MutationFailure, ReadFailure},
};

/// The kernel's own abort releases the blocked caller; this errno only ends
/// the request's reply ownership.
pub(super) const STOPPED: Errno = Errno::ENOTCONN;

/// The one reply of a failed request: stopped, a stale serial, or a failure
/// of its own.
pub(super) fn errno(fenced: bool, stale: bool) -> Errno {
    if fenced {
        STOPPED
    } else if stale {
        Errno::ESTALE
    } else {
        Errno::EIO
    }
}
pub(super) async fn read(fence: &Fence, failure: ReadFailure) -> RequestDisposition {
    if failure.fenced() {
        fence.replied();
    } else if failure.base_demand().is_none() && !failure.stale() {
        return RequestDisposition::Retained(Box::new(failure));
    }
    match failure.relinquish().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => RequestDisposition::Retained(Box::new(failure)),
    }
}
/// Neither kind of failure follows a publication; a mutation that holds a
/// ticket is retained by its relinquisher.
pub(super) async fn mutation(fence: &Fence, failure: MutationFailure) -> RequestDisposition {
    if failure.fenced() {
        fence.replied();
    } else if failure.base_demand().is_none() {
        return coherence::retained(failure);
    }
    match failure.relinquish().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => coherence::retained(failure),
    }
}
pub(super) async fn directory(fence: &Fence, failure: DirectoryFailure) -> RequestDisposition {
    if failure.fenced() {
        fence.replied();
    } else if failure.base_demand().is_none() && !failure.stale() {
        return RequestDisposition::Retained(Box::new(failure));
    }
    match failure.relinquish().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => RequestDisposition::Retained(Box::new(failure)),
    }
}
