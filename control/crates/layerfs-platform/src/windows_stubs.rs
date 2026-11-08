//! Windows stubs for the platform crate's setup calls: same names and
//! signatures as the unix side, each answering the missing-refusal.
//! The machine crate and the API compile against these; nothing runs.
#![cfg(windows)]

use crate::error::MachineError;
use crate::platform::{HostOs, Platform};
use crate::spec::{Bundle, Forward, Running, Spec, Tools};
use crate::Probe;

fn unsupported(what: &str) -> MachineError {
    MachineError::missing(
        format!("the {what} machine adapter does not run on this host"),
        vec!["host:machine-adapter".into()],
    )
}

pub fn create(_spec: &Spec, _bundle: &Bundle) -> Result<Running, MachineError> {
    Err(unsupported("create"))
}
pub fn connect(_spec: &Spec, _bundle: &Bundle) -> Result<Running, MachineError> {
    Err(unsupported("connect"))
}
pub fn start(_spec: &Spec, _bundle: &Bundle) -> Result<Option<Running>, MachineError> {
    Err(unsupported("start"))
}
pub fn stop(_spec: &Spec) -> Result<(), MachineError> {
    Err(unsupported("stop"))
}
pub fn remove(_spec: &Spec) -> Result<(), MachineError> {
    Err(unsupported("remove"))
}
pub fn load_image(_spec: &Spec, _archive: &std::path::Path) -> Result<String, MachineError> {
    Err(unsupported("image load"))
}
pub fn host_identity(_spec: &Spec) -> Result<String, MachineError> {
    Err(unsupported("host identity"))
}
pub fn reach(_spec: &Spec, _url: &str) -> Result<String, MachineError> {
    Err(unsupported("reach"))
}
pub fn forward(_spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
    Err(unsupported("forward"))
}
pub fn forward_close(_spec: &Spec, _forward: &Forward) {}
pub fn copy_in(
    _spec: &Spec,
    _source: &std::path::Path,
    _target: &str,
) -> Result<(), MachineError> {
    Err(unsupported("copy in"))
}
pub fn copy_out(_spec: &Spec, _source: &str, _file: &std::path::Path) -> Result<(), MachineError> {
    Err(unsupported("copy out"))
}
pub fn status(_spec: &Spec) -> Result<(), MachineError> {
    Err(unsupported("status"))
}
pub fn status_within(
    _spec: &Spec,
    _deadline: std::time::Instant,
) -> Result<(), MachineError> {
    Err(unsupported("status within"))
}
pub fn probe(_kind: Platform, _tools: &Tools) -> Probe {
    Probe {
        kind: _kind,
        ready: false,
        missing: vec!["host:machine-adapter".into()],
        version: None,
    }
}
pub use crate::support::{sandbox_allowed, support, StoreKind, SupportRow, Supports};
