//! Concrete ordinary runtime backends and the enum that selects one.
pub mod docker;
#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod native;
pub mod runtime;
pub use runtime::{Runtime, Sandbox};
