//! Linux platform facts for the shared local lifecycle: the `mountinfo`
//! table (process facts from `/proc` are `layerfs_bridge::process`,
//! layerfs-project#112). Linux starts no native Sandbox (#533 O27); the
//! namespaces backend reads these facts for its overlay's filesystem, and a
//! removal reads the mounts under a folder before it deletes it. Only facts
//! live here; the Sandbox lifecycle is shared with macOS
//! in [`super`]. `seccomp` is the filter every namespaces command runs
//! under and the helper's fork into a cgroup, the crate's one audited
//! `unsafe` module. `confined` is the open
//! and `stat` of an inspection read (layerfs-project#178). `net` is a
//! Sandbox's own network namespace, veth pair and LayerFS's firewall table.
pub(crate) mod confined;
pub(crate) mod mounts;
pub(crate) mod net;
pub(crate) mod seccomp;
