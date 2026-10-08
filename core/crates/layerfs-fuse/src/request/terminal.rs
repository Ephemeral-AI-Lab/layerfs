//! The one terminal reply of an admitted request that its mount's stopped
//! fence refused before an attempt.
//!
//! Forced teardown stops the fence after its abort write. A request waiting
//! for owner admission or a Store reader then fails with `Fenced` instead of
//! attempting anything. The caller makes its single reply attempt with
//! [`STOPPED`]; the functions here count that attempt and release what the
//! request still owns through the disposal ports, which the fence never
//! refuses. A request whose release fails is retained with what remains, as
//! any other failed release. A job already submitted is never ended here: its
//! request receives the original result first.
use crate::{
    coherence::reply_order,
    operations::{DirectoryFailure, MutationFailure, ReadFailure},
    ports::{Fence, Fenced, ServiceError},
    RequestDisposition,
};
use fuser::Errno;

/// The kernel's own abort releases the blocked caller; this errno only ends
/// the request's reply ownership.
pub(super) const STOPPED: Errno = Errno::ENOTCONN;

pub(super) fn fenced(reason: &ServiceError) -> bool {
    reason.is::<Fenced>()
}
/// A fenced request that had acquired nothing.
pub(super) fn unowned(fence: &Fence) -> RequestDisposition {
    fence.replied();
    RequestDisposition::Complete
}
pub(super) async fn read(fence: &Fence, failure: ReadFailure) -> RequestDisposition {
    fence.replied();
    match failure.relinquish().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => RequestDisposition::Retained(Box::new(failure)),
    }
}
/// A fenced mutation published nothing; one that holds a ticket is retained.
pub(super) async fn mutation(fence: &Fence, failure: MutationFailure) -> RequestDisposition {
    fence.replied();
    match failure.relinquish().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => reply_order::retained(failure),
    }
}
pub(super) async fn directory(fence: &Fence, failure: DirectoryFailure) -> RequestDisposition {
    fence.replied();
    match failure.relinquish().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => RequestDisposition::Retained(Box::new(failure)),
    }
}
