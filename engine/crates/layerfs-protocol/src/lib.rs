//! How LayerFS processes talk: the SDK call shape and the values the SDK
//! shows (`sdk`), the daemon control wire (`daemon`), authenticated bounded
//! duplex channels (`duplex`) and Store provisioning records (`provision`).
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#[cfg(feature = "duplex")]
pub mod duplex;
mod wire;

pub mod sdk;

pub use provision::daemon_setup::{KernelRoute, StoreKind};
pub use provision::store::{DaemonStore, StoreAuthority};
pub mod daemon;
pub mod initial_record;
pub mod provision;
