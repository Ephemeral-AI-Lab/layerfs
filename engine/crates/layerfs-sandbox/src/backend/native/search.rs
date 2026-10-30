//! The search path a local daemon and its commands start with: a container's
//! on Linux, this process's own on macOS (the native route).
use std::ffi::OsString;

/// A container's search path: what a local daemon and its commands start with
/// on Linux, where the local runtime stands in for a container.
const CONTAINER_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// The search path a local daemon and its commands start with, before a
/// command's own environment: on Linux [`CONTAINER_PATH`], whatever this
/// process has.
#[cfg(not(target_os = "macos"))]
pub(super) fn search_path() -> OsString {
    OsString::from(CONTAINER_PATH)
}

/// The search path a local daemon and its commands start with, before a
/// command's own environment: on macOS this process's own `PATH`; without
/// one, or an empty one, [`CONTAINER_PATH`] and Homebrew's `/opt/homebrew/bin`.
#[cfg(target_os = "macos")]
pub(super) fn search_path() -> OsString {
    match std::env::var_os("PATH") {
        Some(path) if !path.is_empty() => path,
        _ => OsString::from(format!("{CONTAINER_PATH}:/opt/homebrew/bin")),
    }
}
