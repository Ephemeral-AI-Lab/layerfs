//! Bounded diagnostic encoding, delivery and owned retention.
mod encode;
mod queue;
mod retention;
pub use encode::{encode_operation, encode_resource, Identity};
pub use queue::{Loss, Output, OutputConfig, OutputMode};
mod collector;
pub use collector::{Collector, Producer};
