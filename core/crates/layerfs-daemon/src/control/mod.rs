//! Bounded native control over a direct Store and one local overlay owner.
mod failure;
mod operations;
mod registry;
mod serve;
mod status;
mod types;
pub use registry::Service;
pub use serve::{ServeCause, ServeFailure, Served};
pub use types::{Failure, Success};
