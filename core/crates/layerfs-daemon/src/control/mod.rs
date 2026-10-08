//! Bounded native control over a direct Store and one local overlay owner.
#[cfg(target_os = "linux")]
mod attach;
mod cleanup;
#[cfg(target_os = "linux")]
mod detach;
mod failure;
mod force;
mod native;
mod observation;
mod operations;
mod registry;
mod serve;
#[cfg(target_os = "linux")]
mod serving;
mod status;
mod types;
mod unmount;
pub use native::{NativeConfig, NativeServing};
pub(crate) use observation::{reply_token, request_token};
pub use registry::Service;
pub(crate) use serve::answer_call;
pub use serve::{ServeCause, ServeFailure, Served};
pub use types::{Failure, NativeEvidence, NativeFailure, Success};
