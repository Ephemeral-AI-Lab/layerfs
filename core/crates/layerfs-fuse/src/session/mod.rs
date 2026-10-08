//! Native connection ownership: attach evidence, serving facts and drain.
mod drain;
mod startup;
mod state;

pub use drain::Detach;
pub use state::{
    AttachFailure, AttachPhase, AttachRemainder, DrainStage, Drained, NativeSession, SessionConfig,
    SessionFacts, SessionObserver, Undrained, RECEIVE_LOOPS,
};
