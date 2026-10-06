//! Original logical framing failures; no replay or provider disposition.
use std::fmt;
/// A logical record was refused before any runtime adapter invocation.
#[derive(Debug)]
pub enum FrameError {
    /// Invalid framing, sequence, class, identity or declared range.
    Invalid(&'static str),
    /// Configured live processing credit is unavailable.
    AdmissionUnavailable,
    /// Original allocation request could not be satisfied.
    Allocation {
        /// Original requested body/record capacity.
        requested_bytes: usize,
        /// Exact original allocator refusal, without retry or substitution.
        error: std::collections::TryReserveError,
    },
    /// A logical message identifier's physical integer space is exhausted.
    IdentityExhausted,
    /// A transport ownership mutex was poisoned; it is not repaired/retried.
    Poisoned,
    /// Canonical reply bytes do not match their authenticated claimed identity.
    IdentityMismatch,
}
impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "logical frame: {self:?}")
    }
}
impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation { error, .. } => Some(error),
            _ => None,
        }
    }
}
/// Result of one checked logical record operation.
pub type FrameResult<T> = Result<T, FrameError>;
