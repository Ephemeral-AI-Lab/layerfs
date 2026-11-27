//! The one extension point for a platform (#535 §6.2, G2): a crate-private,
//! sealed trait with no default methods, reached through [`adapter`], the
//! one exhaustive match. Each platform folder holds its `ADAPTER`. The
//! public calls in `setup/calls.rs` dispatch through it; a platform that
//! cannot do an operation answers with that operation's refusal.
use crate::platform::error::{Code, MachineError};
use crate::table::profile::{HostOs, Platform};
use crate::platform::{apple_container, docker, host, lima};
use crate::setup::guest::Guest;
use crate::setup::{State, Status};
use crate::setup::spec::{Bundle, Forward, NameLimit, Running, Spec, Tools};

/// What a platform does; implemented once per platform folder.
pub(crate) trait Adapter: Sync {
    /// What the platform lacks on `host` (pushed onto `missing`) and its
    /// tool's version when it answers. Starts nothing.
    fn probe(&self, tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String>;
    /// How the host reaches a running machine's guest.
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError>;
    /// The provider's own limit on a VM's name on this host, from its
    /// settings alone; `None`: only the plain-name rule. Asks the provider
    /// nothing.
    fn name_limit(&self) -> Option<NameLimit>;
    /// The provider's name of the volume or disk that holds the machine's
    /// data (`/layerfs/data`), from the spec alone; `None`: the machine
    /// has none of its own. Asks the provider nothing.
    fn data_volume(&self, spec: &Spec) -> Option<String>;
    /// Makes the machine and starts its worker.
    fn create(&self, spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError>;
    /// Deletes the machine.
    fn remove(&self, spec: &Spec) -> Result<(), MachineError>;
    /// Starts a stopped machine or its worker; `None`: nothing to start.
    fn start(&self, spec: &Spec, bundle: &Bundle) -> Result<Option<Running>, MachineError>;
    /// Stops the worker and, where there is one, the VM.
    fn stop(&self, spec: &Spec, force: bool) -> Result<(), MachineError>;
    /// Provider runtime state only, without worker contact or guest commands.
    fn provider_state(&self, spec: &Spec) -> Result<State, MachineError>;
    /// The machine's state now; starts nothing.
    fn status(&self, spec: &Spec) -> Result<Status, MachineError>;
    /// How the machine's Sandboxes reach the caller's loopback, which
    /// `crate::reach` checks every PostgreSQL Store URL against. Reads
    /// nothing from the machine.
    fn loopback(&self, spec: &Spec) -> crate::table::reach::Loopback;
    /// The host loopback address at which this process reaches a Sandbox
    /// daemon listening on `127.0.0.1:<guest_port>` in the machine (the
    /// worker's network namespace), or why the machine forwards none
    /// (`setup::forward::refuse`). One attempt; nothing is retried.
    fn forward(&self, spec: &Spec, guest_port: u16) -> Result<Forward, MachineError>;
}

/// The platform's adapter: the one match over platforms for what they do.
pub(crate) fn adapter(platform: Platform) -> &'static dyn Adapter {
    match platform {
        Platform::Host => &host::ADAPTER,
        Platform::Docker => &docker::ADAPTER,
        Platform::AppleContainer => &apple_container::ADAPTER,
        Platform::Lima => &lima::ADAPTER,
    }
}

/// An operation the machine's platform does not do.
pub(crate) fn refused(spec: &Spec, why: &str) -> MachineError {
    MachineError::new(
        Code::InvalidInput,
        format!("the {} machine {} {why}", spec.kind.name(), spec.name),
    )
}
