//! Linux platform facts for the shared local lifecycle: the `mountinfo`
//! table (process facts from `/proc` are `layerfs_bridge::process`,
//! layerfs-project#112). Linux starts no native Sandbox (#533 O27); the
//! namespaces backend reads these facts for its overlay's filesystem, and a
//! native Sandbox recorded before 2.0 has its mounts detached lazily with
//! them. Only facts live here; the Sandbox lifecycle is shared with macOS
//! in [`super`]. `seccomp` is the filter every namespaces command runs
//! under, the crate's one audited `unsafe` module.
pub(crate) mod mounts;
pub(crate) mod seccomp;
