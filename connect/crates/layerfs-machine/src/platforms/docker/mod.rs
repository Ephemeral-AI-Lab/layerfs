//! `docker`: the built-in machine whose kernel is Docker's (Docker Desktop's
//! VM, or the Linux host). On macOS and Linux its worker is one detached
//! container behind a secret on a loopback-published port, which opens its
//! sqlite Stores in a Docker volume (DECISIONS, H §7.7 row). Docker through
//! WSL2 (Windows) is not designed and answers `Unsupported`.
mod cli;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod network;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod power;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod windows;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use power::{start, stop};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use unix::{guest, status};
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) use windows::{guest, start, status, stop};
