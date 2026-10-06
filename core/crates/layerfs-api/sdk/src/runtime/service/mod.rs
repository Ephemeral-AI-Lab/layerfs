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
mod input;
pub use input::{InputEvent, InputFailure, InputFence, InputPool, InputReport, NativeInput};
mod output;
pub use output::{
    NativeOutput, OutputAdmissionError, OutputConfig, OutputFence, OutputPacket, OutputPool,
    OutputReceipt, OutputReport, OutputStartError, OutputWork,
};
