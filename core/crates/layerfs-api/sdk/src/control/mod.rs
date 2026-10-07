//! One authenticated control connection, with original one-attempt custody.
mod connection;
pub use connection::{Control, ControlCause, ControlFailure, ControlPhase};
