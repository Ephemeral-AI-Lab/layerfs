//! Linux platform facts for the shared local lifecycle: process facts from
//! `/proc` and the `mountinfo` table. Linux starts no native Sandbox (#533
//! O27); the namespaces backend reads these facts for its init process and
//! its overlay's filesystem, and a native Sandbox recorded before 2.0 is
//! stopped and its mounts detached lazily with them. Only facts live here;
//! the Sandbox lifecycle is shared with macOS in [`super`]. `seccomp` is
//! the filter every namespaces command runs under, the crate's one audited
//! `unsafe` module on Linux.
pub(crate) mod mounts;
mod process;
pub(crate) mod seccomp;
pub(crate) use process::{boot, processes, stat};
