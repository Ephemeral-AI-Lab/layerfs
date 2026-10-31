//! Linux platform facts for the native runtime: the daemon runs as root and
//! serves FUSE, so its process facts come from `/proc` and its mounts are
//! detached lazily. Only facts live here; the Sandbox lifecycle is shared
//! with macOS in [`super::native`].
pub(crate) mod mounts;
mod process;
pub(crate) use process::{boot, processes, stat};
