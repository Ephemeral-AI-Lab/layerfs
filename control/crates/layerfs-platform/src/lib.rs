//! Machines (host only): the kernels Sandboxes run on (H §7.1, §7.10.1). One
//! adapter per kind makes and starts its kernel and runs setup steps 1 to 4
//! (VM and data disk, bundle, worker, forward and readiness), returning the
//! worker's endpoint; it knows nothing of the LayerFS home, so the caller
//! passes every path and setting and writes every record.
//!
//! The platform table lives once, in `platform.rs`: one [`Profile`] per
//! platform (its `PROFILE` in `platforms/<p>/profile.rs`), read through
//! [`Platform::profile`]; every refusal of a platform, host and Store kind
//! comes from the one support table, `support.rs`. This file only declares. This layer uses only
//! `layerfs-sandbox`'s shared types, its `json`, and nothing of `backend/`;
//! `backend/` never uses it.
// Windows compiles the pure surface (errors, options, the platform and
// support tables, specs, facts, reach): the machine adapters run host
// CLIs and guest shells that are unix machinery today; their modules stay
// unix-gated and a Windows machine adapter is the #454 follow-up.
#![forbid(unsafe_code)]
#[cfg(unix)]
mod adapter;
mod error;
mod facts;
mod options;
mod platform;
// The setup stub face lives in stubs.rs; probe's Windows twin lives in
// windows_stubs.rs with it.
#[cfg(windows)]
mod windows_stubs;
#[cfg(windows)]
pub use windows_stubs::probe;
pub(crate) mod platforms;
#[cfg(unix)]
mod probe;
#[cfg(unix)]
mod reach;
#[cfg(unix)]
mod setup;
pub(crate) mod spec;
pub(crate) mod support;
pub use error::{Code, MachineError};
pub use layerfs_sandbox_backend::platform_policy;
pub use layerfs_sandbox_backend::Backend;
#[cfg(any())]
pub use layerfs_sandbox::Backend;
pub use options::{OptKind, OptSpec};
pub use platform::{ExecutorKind, HostOs, How, KernelRoute, Platform, Profile, Route};
pub use spec::{
    parse_size, Bundle, Endpoint, Forward, Lasting, Running, Spec, Tools, BUNDLE, DATA,
    IMAGE, STATE, WORKER_PORT,
};
pub use support::{
    platforms, sandbox_allowed, support, support_json, CallSite, PostgresqlSupport, StoreKind,
    SupportRow, Supports,
};
/// One kind's readiness - pure data, every host (unix; Windows reads
/// the stub face until the platform port, #454).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub kind: Platform,
    pub ready: bool,
    pub missing: Vec<String>,
    /// The provider tool's version, when it answers.
    pub version: Option<String>,
}

#[cfg(unix)]
pub use probe::{native_missing, nested_missing, probe as unix_probe};

/// Non-unix: every platform reads not-ready with the one reason; nothing
/// is probed and no tool runs.

/// Non-unix root twins of the setup calls: same names and signatures,
/// each answering the stub refusal.
#[cfg(not(unix))]
pub use stubs::{
    connect as connect, copy_in as copy_in, copy_out as copy_out, create as create,
    forward as forward, forward_close as forward_close, host_identity as host_identity,
    load_image as load_image, reach as reach, remove as remove, start as start,
    status as status, status_within as status_within, stop as stop,
};

#[cfg(unix)]
pub use setup::{
    connect, copy_in, copy_out, create, forward, forward_close, host_identity, load_image, reach,
    remove, start, status, status_within, stop, State, Status,
};

/// The Windows stubs: a Windows host runs no machine adapter today (H
/// 7.10.1 leaves its cfg and dependency plan open; #454). Every machine
/// call answers the crate's own missing-refusal, so callers refuse
/// with the reason instead of failing to build.
#[cfg(not(unix))]
pub mod stubs {
    use super::error::MachineError;
    use super::spec::{Bundle, Forward, Running, Spec};
    use std::path::Path;
    use std::time::Instant;

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
    pub fn load_image(_spec: &Spec, _archive: &Path) -> Result<String, MachineError> {
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
        _source: &Path,
        _target: &str,
    ) -> Result<(), MachineError> {
        Err(unsupported("copy in"))
    }
    pub fn copy_out(_spec: &Spec, _source: &str, _file: &Path) -> Result<(), MachineError> {
        Err(unsupported("copy out"))
    }
    pub fn status(_spec: &Spec) -> Result<super::stubs::Status, MachineError> {
        Err(unsupported("status"))
    }
    pub fn status_within(_spec: &Spec, _deadline: Instant) -> Result<super::stubs::Status, MachineError> {
        Err(unsupported("status within"))
    }


/// The status answer shape: on Windows nothing ever answers, so the
/// struct exists only to type the stub signatures; its state reads
/// missing, the one honest answer.
#[derive(Clone, Debug)]
pub struct Status {
    pub state: StubState,
    pub free_disk: Option<u64>,
    pub tool_version: Option<String>,
    pub down_reason: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct StubState;

impl StubState {
    pub fn as_str(&self) -> &'static str {
        "missing"
    }
}
}
