//! Exact borrowed framing and credited bounded reassembly.
mod credits;
mod frame;
mod receive;
mod types;
pub use credits::ReceiveBudget;
pub use frame::{decode, encode};
pub use receive::Reassembly;
pub use types::{Message, MessageLease, ReassemblyConfig, ReassemblyWork};
