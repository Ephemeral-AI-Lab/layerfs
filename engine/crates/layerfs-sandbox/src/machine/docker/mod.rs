//! `docker`: the built-in machine whose kernel is Docker's (Docker Desktop's
//! VM, or the Linux host). On Linux its Store calls run in the caller's
//! process and nothing is set up; on macOS its worker is one detached
//! container behind a secret on a loopback-published port. Docker through
//! WSL2 (Windows) is not designed and answers `Unsupported`.
mod cli;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod windows;
#[cfg(target_os = "linux")]
pub(crate) use linux::{guest, start, status, stop};
#[cfg(target_os = "macos")]
pub(crate) use macos::{guest, start, status, stop};
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) use windows::{guest, start, status, stop};
