//! Fair bounded jobs over an initialized borrowed serving registry.
mod credits;
mod execution;
mod owner;
mod queue;
mod receipts;
mod types;

pub use owner::Service;
pub use receipts::{ServiceCompletion, ServiceOutcome};
pub use types::{
    ConnectionId, DisconnectFence, ObjectValue, Request, Response, ServiceClass, ServiceConfig,
    ServiceWork, Ticket,
};
