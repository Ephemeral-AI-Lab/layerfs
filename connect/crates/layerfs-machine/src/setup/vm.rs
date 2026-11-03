//! What the VM adapters share: sizes as provider CLIs write them, the data
//! volume's name, the worker's host port, and one provider call.
use crate::error::{Code, MachineError};
use crate::setup::tool::{self, Output};
use crate::spec::Spec;
use std::time::Duration;

pub(crate) const CALL: Duration = Duration::from_secs(30);
pub(crate) const CREATE: Duration = Duration::from_secs(120);
pub(crate) const BOOT: Duration = Duration::from_secs(300);

/// A size in whole MiB as `<n>M`, the form today's settings use.
pub(crate) fn mib(bytes: u64) -> String {
    format!("{}M", bytes >> 20)
}

/// The data volume a container-shaped VM keeps across stop and start.
pub(crate) fn data_volume(spec: &Spec) -> String {
    format!("{}-data", spec.vm_name)
}

/// The host port a new VM's configuration forwards: the one the caller
/// recorded, else a free one.
pub(crate) fn new_port(spec: &Spec) -> Result<u16, MachineError> {
    spec.state.port.map_or_else(tool::free_port, Ok)
}

/// The host port a VM was made with, from its lasting state.
pub(crate) fn recorded_port(spec: &Spec) -> Result<u16, MachineError> {
    spec.state.port.ok_or_else(|| {
        MachineError::new(
            Code::InvalidInput,
            format!("the machine {} has no recorded worker port", spec.name),
        )
    })
}

/// One call of the kind's provider CLI.
pub(crate) fn call(
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
pub(crate) fn checked(
    spec: &Spec,
    cli: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, MachineError> {
    call(spec, cli, args, timeout)?.answer(&format!("{cli} {}", args.first().unwrap_or(&"")))
}

/// Refuses a new machine whose VM name the provider already holds.
pub(crate) fn exists(spec: &Spec, what: &str) -> MachineError {
    MachineError::new(
        Code::Exists,
        format!("{what} {} already exists; it is left alone", spec.vm_name),
    )
}

/// `NotFound` for a start on a VM the provider does not hold.
pub(crate) fn not_found(spec: &Spec) -> MachineError {
    MachineError::new(
        Code::NotFound,
        format!(
            "the {} VM {} does not exist",
            spec.kind.name(),
            spec.vm_name
        ),
    )
}
