//! Machines (host only): the kernels Sandboxes run on (H §7.1, §7.10.1). One
//! adapter per kind makes and starts its kernel and runs setup steps 1 to 4
//! (VM and data disk, bundle, worker, forward and readiness), returning the
//! worker's endpoint; it knows nothing of the LayerFS home, so the caller
//! passes every path and setting and writes every record.
//!
//! The kind-to-backend table lives once, in `kind.rs` ([`Kind::backend`],
//! [`Kind::route`], [`Kind::worker_bind`]); this file only declares. There
//! is no provider trait: [`create`], [`start`] and the other calls match
//! once on the kind. This layer uses only `layerfs-sandbox`'s shared types,
//! its `json`, and nothing of `backend/`; `backend/` never uses it.
#![cfg(unix)]
#![forbid(unsafe_code)]
mod error;
mod facts;
mod kind;
pub(crate) mod platforms;
mod probe;
mod setup;
mod spec;
pub use error::{Code, MachineError};
pub use kind::{HostOs, Kind, Route};
pub use layerfs_sandbox::Backend;
pub use probe::{native_missing, nested_missing, probe, Probe};
pub use setup::{
    connect, copy_in, copy_out, create, host_identity, remove, start, status, status_within, stop,
    State, Status,
};
pub use spec::{
    parse_size, Bundle, Endpoint, Lasting, Running, Spec, SshSpec, Tools, BUNDLE, DATA, IMAGE,
    STATE, WORKER_PORT,
};
