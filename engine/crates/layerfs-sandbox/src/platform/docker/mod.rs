//! Local Docker Engine API1.48 to 1.54, negotiated once, with exact IDs and no command replay.
//! One container per Sandbox: `create` starts it, `sandbox` owns it (stop,
//! delete), `exec` runs one command in it, and `client/` is the Engine HTTP
//! client the rest reads.
mod client;
mod create;
mod endpoint;
mod exec;
mod failure;
mod listener;
mod provider;
mod sandbox;
mod streams;
mod topology;
mod upload;
pub use client::Docker;
pub(crate) use exec::run as run_command;
pub use exec::{ExecCreateFailure, ExecCreated, ExecRequest};
pub use failure::ProviderFailure;
pub use listener::{ListenerFailure, ListenerObservation};
pub use provider::DockerProvider;
pub use sandbox::{CreatePhase, Sandbox, SandboxCreateFailure, SandboxDisposition, SandboxRequest};
pub use streams::{
    Attached, ExecHandle, InputCloseFailure, InputEnd, OutputFailure, OutputProgress, RuntimeInput,
    RuntimeOutput, SplitFailure, StartFailure,
};
pub use upload::{UploadFailure, UploadWork};
