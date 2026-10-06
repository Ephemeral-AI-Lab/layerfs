//! Logical transport record and ownership contracts.
mod error;
mod frame;
pub use error::{FrameError, FrameResult};
pub use frame::{
    Envelope, Fragment, MessageClass, MessageKind, HEADER_BYTES, MAX_FRAGMENT_BYTES,
    MAX_RECORD_BYTES,
};
