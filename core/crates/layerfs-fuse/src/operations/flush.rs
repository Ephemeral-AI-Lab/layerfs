//! FLUSH, FSYNC and FSYNCDIR.
//!
//! Kernel writeback caching is not negotiated, so every `write(2)` has already
//! been published by its own WRITE before it returned, and a mapped store is
//! published by the WRITE the kernel issues before the FSYNC of an `msync`.
//! Nothing is buffered in the daemon for these requests to push.

/// What a synchronization request does here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Synchronize {
    /// Success with no engine job, no Store seal and no durability claim: the
    /// Overlay is disposable state and is never synchronized to stable media.
    Acknowledge,
}
/// FLUSH on every close, and FSYNC/FSYNCDIR with or without the data-only
/// flag, are the same acknowledgement.
pub const fn synchronize() -> Synchronize {
    Synchronize::Acknowledge
}
