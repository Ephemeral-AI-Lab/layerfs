//! Linux platform facts, which only the namespaces backend's host has: the
//! `mountinfo` table (process facts from `/proc` are
//! `layerfs_bridge::process`, layerfs-project#112). Linux starts no native
//! Sandbox (#533 O27); the namespaces backend reads these facts for its
//! overlay's filesystem, and a removal reads the mounts under a folder
//! before it deletes it. Only facts live here; the daemon lifecycle both
//! backends use is `crate::shared`, which reaches this module as `os`.
//! `confined` is the open and `stat` of an inspection read
//! (layerfs-project#178). The command filter (`super::seccomp`, the crate's
//! one audited `unsafe` module) and a Sandbox's own network
//! (`super::network`) are the backend's siblings of this folder.
pub(crate) mod confined;
pub(crate) mod mounts;
