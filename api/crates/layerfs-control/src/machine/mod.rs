//! The `machine` family (H §7.1, §7.2): records in `machine/<name>/`, the
//! built-in `native` and `docker`, VMs created by name and ssh hosts
//! connected. The adapters that make and start machines are
//! `layerfs-sandbox`'s `machine/` layer; this module writes the records and
//! keeps `Exists` and `InUse`. Without the `machine` feature (the guest
//! worker's build) every machine call is `Unsupported`.
#[cfg(feature = "machine")]
mod bundle;
#[cfg(feature = "machine")]
mod calls;
#[cfg(feature = "machine")]
mod platforms;
#[cfg(feature = "machine")]
mod record;
#[cfg(feature = "machine")]
mod start;

#[cfg(feature = "machine")]
pub(crate) use bundle::bundle;
#[cfg(feature = "machine")]
pub use calls::{connect, create, disconnect, get, list, remove, status, update};
#[cfg(feature = "machine")]
pub use platforms::platforms;
#[cfg(feature = "machine")]
pub use record::{ensure_built_ins, kind_of as platform_of, machine_of, vm_name};
#[cfg(feature = "machine")]
pub use start::running;
