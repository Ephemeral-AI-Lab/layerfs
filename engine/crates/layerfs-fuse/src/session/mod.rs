//! Kernel connection ownership: attach evidence, serving facts and drain.
mod attach;
mod drain;
mod forced;
mod invalidate;
mod state;

pub use drain::Detach;
pub(crate) use invalidate::Deferred;
pub use invalidate::Invalidator;
pub use state::{
    AttachFailure, AttachPhase, AttachRemainder, DetachAttempt, DrainStage, Drained, ForceRefusal,
    Forced, KernelSession, SessionConfig, SessionFacts, SessionObserver, Undrained, RECEIVE_LOOPS,
};
