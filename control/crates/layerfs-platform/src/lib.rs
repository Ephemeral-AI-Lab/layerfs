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
//! `layerfs-sandbox-backend`'s shared types, its `json`, and nothing of `backend/`;
//! `backend/` never uses it.
#![cfg(unix)]
#![forbid(unsafe_code)]
mod adapter;
mod error;
mod facts;
mod options;
mod platform;
pub(crate) mod platforms;
mod probe;
mod reach;
mod setup;
mod spec;
mod support;
pub use error::{Code, MachineError};
pub use layerfs_sandbox_backend::platform_policy;
pub use layerfs_sandbox_backend::Backend;
pub use options::{OptKind, OptSpec};
pub use platform::{ExecutorKind, HostOs, How, KernelRoute, Platform, Profile, Route};
pub use probe::{native_missing, nested_missing, probe, Probe};
pub use setup::{
    copy_in, copy_out, create, forward, forward_close, load_image, reach, remove, start, status,
    status_within, stop, State, Status,
};
pub use spec::{
    parse_size, Bundle, Endpoint, Forward, Lasting, Running, Spec, Tools, BUNDLE, DATA, IMAGE,
    STATE, WORKER_PORT,
};
pub use support::{
    platforms, sandbox_allowed, support, support_json, CallSite, PostgresqlSupport, StoreKind,
    SupportRow, Supports,
};
