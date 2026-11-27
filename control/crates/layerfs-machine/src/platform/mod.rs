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
pub(crate) mod adapter;
pub(crate) mod apple_container;
#[cfg(feature = "machine")]
pub(crate) mod available;
pub(crate) mod dispatch;
pub(crate) mod docker;
pub(crate) mod error;
pub(crate) mod host;
pub(crate) mod lima;
pub use super::setup::find_tool;
pub use super::setup::probe::{native_missing, probe, Probe};
pub use super::setup::spec::{
    parse_size, search_path, tool_fallbacks, Bundle, Endpoint, Forward, Lasting, NameLimit,
    Running, Spec, Tools, BUNDLE, DATA, IMAGE, STATE, WORKER_LOG, WORKER_PORT,
};
pub use super::setup::{
    cached_image, copy_in, copy_out, create, data_volume, forward, forward_close, load_image,
    loopback_hint, name_limit, provider_state, provider_within, reach, remove, shares_loopback,
    start, status, status_within, stop, stop_force, worker_port, State, Status,
};
pub use super::table::options::{OptKind, OptSpec};
pub use super::table::profile::{ExecutorKind, HostOs, How, Platform, Profile, Route};
pub use super::table::support::{
    platforms, sandbox_allowed, support, support_json, CallSite, PostgresqlSupport, SupportRow,
    Supports,
};
pub use crate::table::backend::Backend;
pub use crate::table::removed as removed_platforms;
pub use docker::socket::{
    docker_automatic, docker_candidates, socket_state, DockerCandidate, DockerEnv, SocketState,
    DEFAULT_DOCKER_SOCKET, SOCKET_ANSWER,
};
pub use error::{Code, MachineError};
pub use layerfs_protocol::KernelRoute;
pub use layerfs_protocol::StoreKind;

// The VM kinds (the port of `api/runtime/src/platform/vm/`): one VM
// per machine, named `<prefix><machine>`, with one data disk at
// `/layerfs/data` holding many Stores and one worker. No guest mounts the
// host home. `crate::setup::vm` holds what the adapters share;
// `crate::setup::forward` is the detached `ssh -L` that `lima` uses.
// `host` and `docker` are the built-in kinds.
