//! Bounded kernel control over a direct Store and one local overlay owner.
#[cfg(target_os = "linux")]
mod attach;
#[cfg(unix)]
mod cleanup;
#[cfg(target_os = "linux")]
mod detach;
#[cfg(unix)]
mod diagnostic;
#[cfg(unix)]
mod dirty;
#[cfg(unix)]
mod export;
#[cfg(unix)]
mod export_helper;
#[cfg(any(unix, windows))]
mod failure;
#[cfg(unix)]
mod force;
#[cfg(unix)]
mod inspect;
#[cfg(unix)]
mod invalidate;
#[cfg(any(unix, windows))]
mod kernel;
#[cfg(windows)]
mod attach_windows;
#[cfg(unix)]
mod observation;
#[cfg(unix)]
mod operations;
#[cfg(unix)]
mod registry;
#[cfg(windows)]
mod registry_windows;
#[cfg(windows)]
mod operations_windows;
#[cfg(any(unix, windows))]
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
#[cfg(unix)]
pub(crate) use observation::{reply_token, request_token};
#[cfg(unix)]
pub use registry::Service;
#[cfg(windows)]
pub use registry_windows::Service;
#[cfg(any(unix, windows))]
pub(crate) use serve::answer_call;
#[cfg(any(unix, windows))]
pub use serve::{ServeCause, ServeFailure, Served};
pub use types::{Failure, KernelEvidence, KernelFailure, Success};
