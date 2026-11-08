//! Exact ordinary runtime identities, independent process/output knowledge and
//! failures, and the launch inputs and observations every backend shares.
pub mod kind;
mod bind;
mod capabilities;
mod execution;
mod failure;
mod identity;
mod image;
mod options;
mod request;
mod transfer;
pub use bind::Bind;
pub use capabilities::{IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};
pub use execution::CommandIdentity;
#[cfg(unix)]
pub(crate) use failure::STOPPED;
pub use failure::{RuntimeError, WireFailure};
pub use identity::ContainerId;
pub use image::Image;
pub use options::{
    ExecExit, LaunchPhase, LaunchRequest, LocalRecord, NamespaceRecord, ProviderLaunchOptions,
    SandboxInspection, SandboxRecord,
};
pub use request::ExecRequest;
pub use transfer::PendingRequest;
/// Compatible name for the existing exact 64-hex runtime identity.
pub type SandboxId = ContainerId;
