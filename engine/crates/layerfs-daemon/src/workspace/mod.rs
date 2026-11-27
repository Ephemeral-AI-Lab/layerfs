//! Bounded kernel control over a direct Store and one local overlay owner.
mod failure;
pub(crate) mod fuse;
mod kernel;
pub(crate) mod nfs;
pub(crate) mod receipts;
mod registry;
pub(crate) mod route;
pub(crate) mod status;
pub(crate) mod teardown;
pub use kernel::{KernelConfig, KernelServing};
pub use nfs::export::{ExportServing, ExportServingConfig, Helper, Mounter, HELPER_WAIT};
pub use receipts::{Failure, KernelEvidence, KernelFailure, Success};
pub use registry::Service;
pub(crate) use route::serve::answer_call;
pub use route::serve::{ServeCause, ServeFailure, Served};
pub(crate) use status::observation::{reply_token, request_token};
