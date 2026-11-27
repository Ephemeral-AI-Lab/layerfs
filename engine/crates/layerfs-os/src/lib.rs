//! What this kernel reports: the local process identity the home and the
//! drivers share (`process`), the macOS mount table under a deadline and
//! bounded host-name resolution (`network`) and the Sandbox runtime layer
//! the process runs under (`runtime`).
// unsafe only in process/darwin.rs (macOS process facts come from
// proc_pidinfo, a system call: `ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#![deny(missing_docs)]
#[cfg(target_os = "macos")]
pub(crate) mod mount_table;
pub mod network;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod process;
pub mod runtime;
