//! Machine setup: the sealed Linux bundle (`bundle`, every build), the
//! docker machine's default image (`default_image`), and, on host builds,
//! the public calls, the guest plumbing (`guest`, `tool`, `worker`) and
//! what the VM adapters share (`forward`, `vm`).
pub mod bundle;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod bundle_install;
#[cfg(feature = "machine")]
pub(crate) mod default_image;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod facts;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod forward;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod guest;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod probe;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod spec;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod tool;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod vm;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod worker;
#[cfg(all(feature = "machine", unix))]
pub(crate) use super::platform::dispatch::with_worker;
#[cfg(all(feature = "machine", unix))]
pub use super::platform::dispatch::{
    cached_image, copy_in, copy_out, create, data_volume, forward, forward_close, load_image,
    loopback_hint, name_limit, provider_state, provider_within, reach, remove, shares_loopback,
    start, status, status_within, stop, stop_force, worker_port, State, Status,
};
#[cfg(all(feature = "machine", unix))]
pub use tool::find_tool;
