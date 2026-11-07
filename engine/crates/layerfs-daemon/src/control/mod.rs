//! Bounded kernel control over a direct Store and one local overlay owner.
#[cfg(target_os = "linux")]
mod attach;
#[cfg(unix)]
mod cleanup;
#[cfg(target_os = "linux")]
mod detach;
#[cfg(unix)]
mod dirty;
#[cfg(unix)]
mod export;
#[cfg(unix)]
mod export_helper;
#[cfg(unix)]
mod failure;
#[cfg(unix)]
mod force;
#[cfg(unix)]
mod inspect;
#[cfg(unix)]
mod invalidate;
#[cfg(unix)]
mod kernel;
#[cfg(unix)]
mod observation;
#[cfg(unix)]
mod operations;
#[cfg(unix)]
mod registry;
#[cfg(unix)]
mod serve;
#[cfg(target_os = "linux")]
mod serving;
#[cfg(unix)]
mod status;
mod types;
#[cfg(unix)]
mod unmount;
#[cfg(unix)]
pub use export::{ExportServing, ExportServingConfig, Helper, Mounter, HELPER_WAIT};
#[cfg(unix)]
pub use kernel::{KernelConfig, KernelServing};
pub(crate) use observation::{reply_token, request_token};
#[cfg(unix)]
pub use registry::Service;
pub(crate) use serve::answer_call;
#[cfg(unix)]
pub use serve::{ServeCause, ServeFailure, Served};
pub use types::{Failure, KernelEvidence, KernelFailure, Success};
