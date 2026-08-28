//! Durable synchronization endpoint contracts.

mod control;
mod read;

pub use control::DurableControlEndpoint;
pub use read::DurableEndpoint;
