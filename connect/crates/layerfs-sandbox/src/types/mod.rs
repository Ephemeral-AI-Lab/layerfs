//! Exact ordinary runtime identities, independent process/output knowledge and
//! failures, and the launch inputs and observations every backend shares.
mod bind;
mod capabilities;
mod execution;
mod failure;
mod identity;
mod image;
mod isolation;
mod options;
mod request;
mod transfer;
pub use bind::Bind;
pub use capabilities::{
    IdleStop, IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy,
    SnapshotCapability, CAPABILITY_SCHEMA_VERSION,
};
pub use execution::{CommandIdentity, ExecInspection};
#[cfg(unix)]
pub(crate) use failure::STOPPED;
pub use failure::{ErrorDiagnostic, RuntimeError, WireFailure};
pub use identity::{ContainerId, ExecId};
pub use image::Image;
pub use isolation::{ExecMode, IsolationClass, KernelScope};
pub use options::{
    DockerRecord, ExecExit, LaunchPhase, LaunchRequest, LocalRecord, NamespaceRecord,
    ProviderLaunchOptions, SandboxInspection, SandboxRecord,
};
pub use request::ExecRequest;
pub use transfer::PendingRequest;
/// Compatible name for the existing exact 64-hex runtime identity.
pub type SandboxId = ContainerId;
