//! The `machine` family (H §7.1, §7.2): records in `machine/<name>/`, the
//! built-in `native` and `docker`, VMs created by name and ssh hosts
//! connected. The adapters that make and start machines are
//! `layerfs-isolation`'s `machine/` layer; this module writes the records and
//! keeps `Exists`, `InUse` and each record's `op` (`Busy`, `Unresolved`). Without the `machine` feature (the guest
//! worker's build) every machine call is `Unsupported`.
#[cfg(feature = "machine")]
mod bundle;
#[cfg(feature = "machine")]
mod connect;
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
#[cfg(feature = "machine")]
mod platforms;
#[cfg(feature = "machine")]
mod power;
#[doc(hidden)]
pub mod record;
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
pub(crate) use bundle::bundle;
#[cfg(feature = "machine")]
pub use connect::connect;
#[cfg(feature = "machine")]
pub use create::create;
#[cfg(feature = "machine")]
pub use endpoint::running;
#[cfg(feature = "machine")]
pub use get::get;
#[cfg(feature = "machine")]
pub(crate) use image::docker_default;
#[cfg(feature = "machine")]
pub use list::list;
#[cfg(feature = "machine")]
pub use platforms::platforms;
#[cfg(feature = "machine")]
pub use power::{start, stop};
#[cfg(feature = "machine")]
pub use remove::{disconnect, remove};
#[cfg(feature = "machine")]
pub use spec::{ensure_built_ins, machine_of, vm_name};
#[cfg(feature = "machine")]
pub(crate) use spec::{refusal as machine_refusal, spec as machine_spec};
#[cfg(feature = "machine")]
pub use status::status;
#[cfg(feature = "machine")]
pub use update::update;
