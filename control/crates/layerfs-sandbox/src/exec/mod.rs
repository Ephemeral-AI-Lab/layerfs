//! Commands in a mounted Workspace (compute-plane 2.0 §5, R9): `run`, the
//! exec itself, with the id that names it (`id`), its record while it runs
//! (`record`) and its cancel (`cancel`, `workspace.cancel`, and what
//! close and unmount do with running commands). The daemon supervises no
//! command: exec and cancel live here and in the Sandbox backend.
pub mod id;
#[doc(hidden)]
pub mod output;
pub(crate) mod pending;
pub mod remote;
pub mod settle;
