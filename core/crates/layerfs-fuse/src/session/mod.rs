//! Native connection ownership: attach evidence, serving facts and drain.
mod drain;
mod force;
mod startup;
mod state;

pub use drain::Detach;
pub use state::{
    AttachFailure, AttachPhase, AttachRemainder, DetachAttempt, DrainStage, Drained, ForceRefusal,
    Forced, NativeSession, SessionConfig, SessionFacts, SessionObserver, Undrained, RECEIVE_LOOPS,
};
