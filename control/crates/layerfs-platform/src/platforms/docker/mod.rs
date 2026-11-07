//! `docker`: the built-in machine whose kernel is Docker's (Docker Desktop's
//! VM, or the Linux host). On macOS and Linux its worker is one detached,
//! privileged container behind a secret on a loopback-published port, which
//! opens its sqlite Stores in a Docker volume (DECISIONS, H §7.7 row) and
//! runs `namespaces` Sandboxes inside itself as a VM's worker does (D-8).
//! Docker through WSL2 (Windows) is not designed and answers `Unsupported`.
mod adapter;
mod cli;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod network;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod power;
mod profile;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod windows;
pub(crate) use adapter::ADAPTER;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use power::{load_image, start, stop};
pub(crate) use profile::PROFILE;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use unix::{guest, host_address, status};
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) use windows::{guest, host_address, load_image, start, status, stop};
