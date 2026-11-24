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
mod docker_socket;
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
pub use docker_socket::{
    docker_automatic, docker_candidates, socket_state, DockerCandidate, DockerEnv, SocketState,
    DEFAULT_DOCKER_SOCKET, SOCKET_ANSWER,
};
pub use error::{Code, MachineError};
pub use layerfs_sandbox_backend::removed_platforms;
pub use layerfs_sandbox_backend::Backend;
pub use options::{OptKind, OptSpec};
pub use platform::{ExecutorKind, HostOs, How, KernelRoute, Platform, Profile, Route};
pub use probe::{native_missing, probe, Probe};
pub use setup::find_tool;
pub use setup::{
    cached_image, copy_in, copy_out, create, data_volume, forward, forward_close, load_image,
    loopback_hint, name_limit, provider_state, provider_within, reach, remove, shares_loopback,
    start, status, status_within, stop, stop_force, worker_port, State, Status,
};
pub use spec::{
    parse_size, search_path, tool_fallbacks, Bundle, Endpoint, Forward, Lasting, NameLimit,
    Running, Spec, Tools, BUNDLE, DATA, IMAGE, STATE, WORKER_LOG, WORKER_PORT,
};
pub use support::{
    platforms, sandbox_allowed, support, support_json, CallSite, PostgresqlSupport, StoreKind,
    SupportRow, Supports,
};
