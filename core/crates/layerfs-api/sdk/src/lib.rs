//! Application-embedded host runtime over the initialized cluster-one provider.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod client;
pub mod runtime;
pub use runtime::{
    Authorization, Binding, BoundLengths, BoundSerials, Completion, CompletionPhase, Config,
    HistoryReceipts, LengthReply, ObjectReply, Runtime, RuntimeError, RuntimeResult, SaveId,
    Sessions, SERIAL_WINDOW,
};
