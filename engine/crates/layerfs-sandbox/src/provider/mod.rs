//! Runtime providers own processes and command streams, never daemon control protocols.
mod capabilities;
mod lifecycle;
mod options;
pub use capabilities::{IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};
pub use lifecycle::{LaunchFailure, LaunchPhase, SandboxInstance, SandboxProvider};
pub use options::{
    DockerRecord, ExecExit, LaunchRequest, LocalRecord, ProviderLaunchOptions, SandboxInspection,
    SandboxRecord,
};
/// Compatible name for the existing exact 64-hex runtime identity.
pub type SandboxId = crate::ContainerId;
