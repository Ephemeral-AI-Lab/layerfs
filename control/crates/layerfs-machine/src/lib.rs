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

pub(crate) mod lifecycle;
/// The platform driver: one adapter per machine kind, for the Sandbox and
/// Store code that reaches a machine's adapters through its machine
/// (layerfs-project#106).
#[cfg(feature = "machine")]
pub mod platform;
pub mod record;
pub mod route;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod setup;
pub mod status;
#[doc(hidden)]
pub mod table;
#[cfg(feature = "machine")]
mod update;
pub(crate) mod users;
#[cfg(feature = "machine")]
pub mod with_users;

#[cfg(feature = "machine")]
pub use lifecycle::boot::{recorded as worker_endpoint, running};
#[cfg(feature = "machine")]
pub use lifecycle::create::{create, MachineCreate, STOCK};
#[cfg(feature = "machine")]
pub use lifecycle::progress::Phases;
#[cfg(feature = "machine")]
pub use lifecycle::remove::remove;
#[cfg(feature = "machine")]
pub use lifecycle::start::{done, start, stop, with_stopped, STOP_PHASES};
#[cfg(feature = "machine")]
pub use platform::available::platforms;
#[cfg(feature = "machine")]
pub use record::adapter_spec::{
    docker_socket, ensure_built_ins, machine_of, tools as machine_tools, vm_name,
};
#[cfg(feature = "machine")]
pub use record::adapter_spec::{
    provider_spec as machine_provider_spec, refusal as machine_refusal, spec as machine_spec,
};
pub use route::runs_here::{in_process, none_in_process, provided, runs_here};
#[cfg(feature = "machine")]
pub use setup::bundle::bundle;
pub use setup::bundle::{bundle_dir, bundle_place, IMAGE_ARCHIVE};
#[cfg(feature = "machine")]
pub use setup::default_image::docker_default;
#[cfg(feature = "machine")]
pub use status::list::{get, list};
#[cfg(feature = "machine")]
pub use update::{update, MachineUpdate};
pub use users::{MachineUsers, Related};

/// The machine lifecycle lock, shared by the spanning forced-stop call.
#[cfg(feature = "machine")]
pub use lifecycle::boot::start_lock as force_stop_lock;
#[cfg(feature = "machine")]
pub use lifecycle::force_stop::force_record;

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::record::machine as record; // #828 compat
#[doc(hidden)]
pub use crate::route as worker;
#[doc(hidden)]
pub use crate::setup::bundle; // #828 compat
#[doc(hidden)]
pub use crate::status::state; // #828 compat // #828 compat
