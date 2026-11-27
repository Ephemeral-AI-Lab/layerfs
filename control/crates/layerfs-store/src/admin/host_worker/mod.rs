//! The host worker's Store side: its server, which answers the native
//! CLI's Store calls on a Unix socket (`server`, `connection`), and its
//! queues for PostgreSQL requests and streamed writes. Built for the host
//! only (the `machine` feature, macOS and Linux).
#[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
pub(crate) mod connection;
#[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
pub(crate) mod postgresql_queue;
#[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
pub mod server;
#[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
pub(crate) mod write_queue;

#[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
pub use server::serve_host;
