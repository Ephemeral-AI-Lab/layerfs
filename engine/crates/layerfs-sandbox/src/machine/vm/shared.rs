//! What the VM adapters share: sizes as provider CLIs write them, the data
//! volume's name, the worker's host port, and one provider call.
use crate::machine::error::{Code, MachineError};
use crate::machine::spec::Spec;
use crate::machine::tool::{self, Output};
use std::time::Duration;

pub(super) const CALL: Duration = Duration::from_secs(30);
pub(super) const CREATE: Duration = Duration::from_secs(120);
pub(super) const BOOT: Duration = Duration::from_secs(300);

/// A size in whole MiB as `<n>M`, the form today's settings use.
pub(super) fn mib(bytes: u64) -> String {
    format!("{}M", bytes >> 20)
}

/// The data volume a container-shaped VM keeps across stop and start.
pub(super) fn data_volume(spec: &Spec) -> String {
    format!("{}-data", spec.vm_name)
}

/// The host port a new VM's configuration forwards: the one the caller
/// recorded, else a free one.
pub(super) fn new_port(spec: &Spec) -> Result<u16, MachineError> {
    spec.state.port.map_or_else(tool::free_port, Ok)
}

/// The host port a VM was made with, from its lasting state.
pub(super) fn recorded_port(spec: &Spec) -> Result<u16, MachineError> {
    spec.state.port.ok_or_else(|| {
        MachineError::new(
            Code::InvalidInput,
            format!("the machine {} has no recorded worker port", spec.name),
        )
    })
}

/// One call of the kind's provider CLI.
pub(super) fn call(
    spec: &Spec,
    cli: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output, MachineError> {
    let command = tool::command(&tool::find(&spec.tools, cli)?, args);
    tool::run(
        command,
        None,
        timeout,
        &format!("{cli} {}", args.first().unwrap_or(&"")),
    )
}

/// A provider call that must succeed; its trimmed stdout.
pub(super) fn checked(
    spec: &Spec,
    cli: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, MachineError> {
    call(spec, cli, args, timeout)?.answer(&format!("{cli} {}", args.first().unwrap_or(&"")))
}

/// Refuses a new machine whose VM name the provider already holds.
pub(super) fn exists(spec: &Spec, what: &str) -> MachineError {
    MachineError::new(
        Code::Exists,
        format!("{what} {} already exists; it is left alone", spec.vm_name),
    )
}

/// `NotFound` for a start on a VM the provider does not hold.
pub(super) fn not_found(spec: &Spec) -> MachineError {
    MachineError::new(
        Code::NotFound,
        format!(
            "the {} VM {} does not exist",
            spec.kind.name(),
            spec.vm_name
        ),
    )
}
