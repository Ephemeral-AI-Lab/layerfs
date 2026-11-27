//! The worker protocol (`layerfs-machine`), the guest worker's server
//! (`serve`) and the host worker's (`layerfs-store`), for the front ends
//! and their tests.
pub(crate) mod connection;
#[cfg(feature = "worker-fault")]
pub mod fault;
pub(crate) mod ops;
pub mod serve;

pub use layerfs_machine::worker::*;
#[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
pub use layerfs_store::admin::host_worker::server::serve_host;
pub use serve::{prepare_network, serve, serve_with_incarnation};
