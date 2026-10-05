//! Scoped host object/Save service; transport and Workspace control are separate.
mod binding;
mod error;
mod ports;
pub(crate) use ports::lengths as length_port;
mod owner;
pub(crate) use ports::serials as serial_port;
mod sessions;
mod types;

pub use binding::{Authorization, Binding};
pub use error::{RuntimeError, RuntimeResult};
pub use length_port::BoundLengths;
pub use owner::{Config, Runtime};
pub use serial_port::{BoundSerials, SERIAL_WINDOW};
pub use sessions::Sessions;
pub use types::{Completion, CompletionPhase, LengthReply, ObjectReply, SaveId};
