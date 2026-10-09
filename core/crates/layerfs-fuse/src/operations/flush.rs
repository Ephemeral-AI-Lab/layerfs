//! FLUSH, FSYNC and FSYNCDIR.
//!
//! Kernel writeback caching is not negotiated, so every `write(2)` has already
//! been published by its own WRITE before it returned, and a mapped store is
//! published by the WRITE the kernel issues before the FSYNC of an `msync`.
//! Nothing is buffered in the daemon for these requests to push, and the
//! Overlay is disposable state that is never synchronized to stable media.

/// What a synchronization request does here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Synchronize {
    /// `ENOSYS`, once per opcode and connection. The kernel records that the
    /// request is not implemented, returns success to the caller of that
    /// `close`, `fsync` or `fdatasync` and of every later one, and stops
    /// sending the request. The caller sees what an acknowledgement with no
    /// work would give it; no engine job, no Store seal and no durability
    /// claim is made either way.
    NotImplemented,
}
/// FLUSH on every close, and FSYNC/FSYNCDIR with or without the data-only
/// flag, get the same answer.
pub const fn synchronize() -> Synchronize {
    Synchronize::NotImplemented
}
