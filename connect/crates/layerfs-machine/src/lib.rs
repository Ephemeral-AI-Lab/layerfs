//! Machines (host only): the kernels Sandboxes run on (H §7.1, §7.10.1). One
//! adapter per kind makes and starts its kernel and runs setup steps 1 to 4
//! (VM and data disk, bundle, worker, forward and readiness), returning the
//! worker's endpoint; it knows nothing of the LayerFS home, so the caller
//! passes every path and setting and writes every record.
//!
//! The platform table lives once, in `platform.rs`: one [`Profile`] per
//! platform (its `PROFILE` in `platforms/<p>/profile.rs`), read through
//! [`Platform::profile`]; this file only declares. This layer uses only
//! `layerfs-sandbox`'s shared types, its `json`, and nothing of `backend/`;
//! `backend/` never uses it.
#![cfg(unix)]
#![forbid(unsafe_code)]
mod error;
mod facts;
mod options;
mod platform;
pub(crate) mod platforms;
mod probe;
mod setup;
mod spec;
pub use error::{Code, MachineError};
pub use layerfs_sandbox::Backend;
pub use options::{OptKind, OptSpec};
pub use platform::{ExecutorKind, HostOs, How, KernelRoute, Platform, Profile, Route};
pub use probe::{native_missing, nested_missing, probe, Probe};
pub use setup::{
    connect, copy_in, copy_out, create, host_identity, remove, start, status, status_within, stop,
    State, Status,
};
pub use spec::{
    parse_size, Bundle, Endpoint, Lasting, Running, Spec, SshSpec, Tools, BUNDLE, DATA, IMAGE,
    STATE, WORKER_PORT,
};
