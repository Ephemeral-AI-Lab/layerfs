//! Authenticated bounded duplex channels and Store provisioning metadata,
//! and the local process identity the home and the drivers share (`process`).
// unsafe only in process/darwin.rs (macOS process facts come from
// proc_pidinfo, a system call: `ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#![deny(missing_docs)]
#[cfg(feature = "duplex")]
pub mod duplex;
mod wire;

pub mod sdk;


pub use provision::store::{DaemonStore, StoreAuthority};
pub use provision::daemon_setup::{KernelRoute, StoreKind};
pub mod initial_record;
pub(crate) mod daemon;
pub mod provision;

