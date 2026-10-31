//! Runtime inputs and observations; concrete backends own their own lifecycle.
mod capabilities;
mod launch;
mod options;
pub use capabilities::{IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};
pub use launch::{LaunchFailure, LaunchPhase};
pub use options::{
    DockerRecord, ExecExit, LaunchRequest, LocalRecord, NamespaceRecord, ProviderLaunchOptions,
    SandboxInspection, SandboxRecord,
};
/// Compatible name for the existing exact 64-hex runtime identity.
pub type SandboxId = crate::ContainerId;
