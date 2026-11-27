//! Machine setup: the public calls (`calls`), the guest plumbing (`guest`,
//! `tool`, `worker`) and what the VM adapters share (`forward`, `vm`).
pub(crate) mod bundle_install;
pub(crate) mod forward;
pub(crate) mod guest;
pub(crate) mod tool;
pub(crate) mod vm;
pub(crate) mod worker;
pub(crate) mod facts;
pub(crate) mod probe;
pub(crate) mod spec;
pub(crate) use super::platform::dispatch::with_worker;
pub use super::platform::dispatch::{cached_image, copy_in, copy_out, create, data_volume, forward, forward_close, load_image, loopback_hint, name_limit, provider_state, provider_within, reach, remove, shares_loopback, start, status, status_within, stop, stop_force, worker_port, State, Status};
pub use tool::find_tool;
