//! Commands in a mounted Workspace (compute-plane 2.0 §5, R9): `run`, the
//! exec itself, with the id that names it (`id`), its record while it runs
//! (`record`) and its cancel (`cancel`, `workspace.exec_cancel`, and what
//! close and unmount do with running commands). The daemon supervises no
//! command: exec and cancel live here and in the Sandbox backend.
pub mod cancel;
pub mod id;
pub mod record;
pub mod remote;
mod run;

pub use run::{exec, Exec};
