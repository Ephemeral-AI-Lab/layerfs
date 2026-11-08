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
mod get;
#[cfg(feature = "machine")]
mod image;
#[cfg(feature = "machine")]
mod list;
mod local;
#[cfg(feature = "machine")]
mod platforms;
#[cfg(feature = "machine")]
mod power;
#[doc(hidden)]
pub mod record;
mod related;
#[cfg(feature = "machine")]
mod remove;
#[cfg(feature = "machine")]
mod spec;
#[cfg(feature = "machine")]
mod status;
#[cfg(feature = "machine")]
mod update;
pub mod worker;

#[cfg(feature = "machine")]
pub use bundle::bundle;
pub use bundle::{bundle_dir, IMAGE_ARCHIVE};
#[cfg(feature = "machine")]
pub use create::{create, MachineCreate};
#[cfg(feature = "machine")]
pub use endpoint::running;
#[cfg(feature = "machine")]
pub use get::get;
#[cfg(feature = "machine")]
pub use image::docker_default;
/// The platform driver, for the Sandbox code that reaches a machine's
/// adapters through its machine (layerfs-project#106).
#[cfg(feature = "machine")]
pub use layerfs_platform as platform;
#[cfg(feature = "machine")]
pub use list::list;
pub use local::runs_here;
#[cfg(feature = "machine")]
pub use platforms::platforms;
#[cfg(feature = "machine")]
pub use power::{start, stop};
pub use related::Related;
#[cfg(feature = "machine")]
pub use remove::remove;
#[cfg(feature = "machine")]
pub use spec::{ensure_built_ins, machine_of, vm_name};
#[cfg(feature = "machine")]
pub use spec::{refusal as machine_refusal, spec as machine_spec};
#[cfg(feature = "machine")]
pub use status::status;
#[cfg(feature = "machine")]
pub use update::{update, MachineUpdate};
