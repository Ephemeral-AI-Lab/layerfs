//! Machine setup: the public calls (`calls`), the guest plumbing (`guest`,
//! `tool`, `worker`) and what the VM adapters share (`forward`, `vm`).
mod artifacts;
mod calls;
pub(crate) mod forward;
pub(crate) mod guest;
pub(crate) mod tool;
pub(crate) mod vm;
pub(crate) mod worker;
pub(crate) use calls::with_worker;
pub use calls::{
    cached_image, copy_in, copy_out, create, data_volume, forward, forward_close, load_image,
    name_limit, provider_within, reach, remove, start, status, status_within, stop, stop_force,
    worker_port, State, Status,
};
pub use tool::find_tool;
