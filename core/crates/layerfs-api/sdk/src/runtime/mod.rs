//! Scoped host object/Save service; transport and Workspace control are separate.
mod binding;
mod handlers;
pub use handlers::history::HistoryReceipts;
pub use handlers::reply::{encode_grant, encode_refusal, encode_reply, WireReply};
pub use handlers::wire::{check_header, decode_request, WireRequest};
mod error;
mod ports;
pub(crate) use ports::lengths as length_port;
mod owner;
mod root_binding;
pub(crate) use ports::serials as serial_port;
/// Bounded fair host adapter dispatch and local disconnect/result custody.
pub mod service;
mod sessions;
/// Host-thread composition of provider service and independently fenced sockets.
pub mod supervisor;
mod types;

pub use binding::{Authorization, Binding};
pub use error::{RuntimeError, RuntimeResult};
pub use length_port::BoundLengths;
pub use owner::{Config, Runtime};
pub use serial_port::{BoundSerials, SERIAL_WINDOW};
pub use sessions::Sessions;
pub use types::{Completion, CompletionPhase, LengthReply, ObjectReply, SaveId};
