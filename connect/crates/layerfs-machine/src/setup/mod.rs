//! Machine setup: the public calls (`calls`), the guest plumbing (`guest`,
//! `tool`, `worker`) and what the VM adapters share (`forward`, `vm`).
mod calls;
pub(crate) mod forward;
pub(crate) mod guest;
pub(crate) mod tool;
pub(crate) mod vm;
pub(crate) mod worker;
pub(crate) use calls::with_worker;
pub use calls::{
    connect, copy_in, copy_out, create, host_identity, remove, start, status, status_within, stop,
    State, Status,
};
