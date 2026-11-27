//! The `machine` family (H §7.1, §7.2): records in `machine/<name>/`, the
//! built-in `native` and `docker`, VMs created by name, and the worker
//! client (cut from `layerfs-control` by
//! layerfs-project#104). The adapters that make and start machines are the
//! `layerfs-platform` driver; this crate writes the records and keeps
//! `Exists`, `InUse` and each record's `op` (`Busy`, `Unresolved`). What a
//! machine holds (its Stores and Sandboxes) comes from the caller
//! ([`Related`]). Without the `machine` feature (the guest worker's build)
//! the machine calls are not built.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bundle;
#[cfg(feature = "machine")]
mod create;
#[cfg(feature = "machine")]
mod endpoint;
#[cfg(feature = "machine")]
mod image;
#[cfg(feature = "machine")]
mod list;
#[cfg(feature = "machine")]
mod live;
mod local;
#[cfg(feature = "machine")]
mod platforms;
#[cfg(feature = "machine")]
mod power;
#[cfg(feature = "machine")]
mod progress;
#[doc(hidden)]
pub mod record;
mod related;
#[cfg(feature = "machine")]
mod remove;
#[cfg(feature = "machine")]
mod spec;
pub mod state;
#[cfg(feature = "machine")]
mod update;
pub mod worker;
pub(crate) mod lifecycle;
pub(crate) mod platform;
pub(crate) mod setup;
pub(crate) mod table;
#[cfg(feature = "machine")]
pub mod with_users;

#[cfg(feature = "machine")]
pub use bundle::bundle;
pub use bundle::{bundle_dir, bundle_place, IMAGE_ARCHIVE};
#[cfg(feature = "machine")]
pub use create::{create, MachineCreate, STOCK};
#[cfg(feature = "machine")]
pub use endpoint::{recorded as worker_endpoint, running};
#[cfg(feature = "machine")]
pub use image::docker_default;
#[cfg(feature = "machine")]
pub use list::{get, list};
pub use local::{in_process, none_in_process, provided, runs_here};
#[cfg(feature = "machine")]
pub use platforms::platforms;
#[cfg(feature = "machine")]
pub use power::{done, start, stop, with_stopped, STOP_PHASES};
#[cfg(feature = "machine")]
pub use progress::Phases;
pub use related::Related;
#[cfg(feature = "machine")]
pub use remove::remove;
#[cfg(feature = "machine")]
pub use spec::{docker_socket, ensure_built_ins, machine_of, tools as machine_tools, vm_name};
#[cfg(feature = "machine")]
pub use spec::{
    provider_spec as machine_provider_spec, refusal as machine_refusal, spec as machine_spec,
};
#[cfg(feature = "machine")]
pub use update::{update, MachineUpdate};

/// The machine lifecycle lock, shared by the spanning forced-stop call.
#[cfg(feature = "machine")]
pub use endpoint::start_lock as force_stop_lock;
