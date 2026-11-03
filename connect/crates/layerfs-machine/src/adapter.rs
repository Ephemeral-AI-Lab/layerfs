//! The one extension point for a platform (#535 §6.2, G2): a crate-private,
//! sealed trait with no default methods, reached through [`adapter`], the
//! one exhaustive match. Each platform folder holds its `ADAPTER`. The
//! public calls in `setup/calls.rs` dispatch through it; a platform that
//! cannot do an operation answers with that operation's refusal.
use crate::error::{Code, MachineError};
use crate::platform::{HostOs, Platform};
use crate::platforms::{apple_container, docker, firecracker, lima, microsandbox, native, ssh};
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Forward, Running, Spec, Tools};

/// What a platform does; implemented once per platform folder.
pub(crate) trait Adapter: Sync {
    /// What the platform lacks on `host` (pushed onto `missing`) and its
    /// tool's version when it answers. Starts nothing.
    fn probe(&self, tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String>;
    /// How the host reaches a running machine's guest.
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError>;
    /// Makes the machine and starts its worker (`ssh`: connects it).
    fn create(&self, spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError>;
    /// Deletes the machine (`ssh`: disconnects it).
    fn remove(&self, spec: &Spec) -> Result<(), MachineError>;
    /// Starts a stopped machine or its worker; `None`: nothing to start.
    fn start(&self, spec: &Spec, bundle: &Bundle) -> Result<Option<Running>, MachineError>;
    /// Stops the worker and, where there is one, the VM.
    fn stop(&self, spec: &Spec) -> Result<(), MachineError>;
    /// The machine's state now; starts nothing.
    fn status(&self, spec: &Spec) -> Result<Status, MachineError>;
    /// The identity of the machine's host (`ssh`).
    fn host_identity(&self, spec: &Spec) -> Result<String, MachineError>;
    /// The PostgreSQL Store URL the machine's Sandbox daemons dial for
    /// `url`: a loopback host becomes the machine's host alias, or is
    /// refused naming what is missing (`crate::reach`). Starts nothing.
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError>;
    /// The host loopback address at which this process reaches a Sandbox
    /// daemon listening on `127.0.0.1:<guest_port>` in the machine (the
    /// worker's network namespace), or why the machine forwards none
    /// (`setup::forward::refuse`). One attempt; nothing is retried.
    fn forward(&self, spec: &Spec, guest_port: u16) -> Result<Forward, MachineError>;
}

/// The platform's adapter: the one match over platforms for what they do.
pub(crate) fn adapter(platform: Platform) -> &'static dyn Adapter {
    match platform {
        Platform::Native => &native::ADAPTER,
        Platform::Docker => &docker::ADAPTER,
        Platform::AppleContainer => &apple_container::ADAPTER,
        Platform::Microsandbox => &microsandbox::ADAPTER,
        Platform::Lima => &lima::ADAPTER,
        Platform::Firecracker => &firecracker::ADAPTER,
        Platform::Ssh => &ssh::ADAPTER,
    }
}

/// An operation the machine's platform does not do.
pub(crate) fn refused(spec: &Spec, why: &str) -> MachineError {
    MachineError::new(
        Code::InvalidInput,
        format!("the {} machine {} {why}", spec.kind.name(), spec.name),
    )
}
