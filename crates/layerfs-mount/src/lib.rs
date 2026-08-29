#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod adapter;
#[cfg(target_os = "linux")]
mod filesystem;
#[cfg(target_os = "linux")]
mod handles;
#[cfg(target_os = "linux")]
mod inode_table;
#[cfg(target_os = "linux")]
mod mount_session;

#[cfg(target_os = "linux")]
pub use adapter::LayerFs;
#[cfg(target_os = "linux")]
pub use mount_session::run_mount;
