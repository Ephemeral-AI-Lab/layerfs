//! Concrete ordinary runtime backends and the enum that selects one.
pub mod docker;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod identity;
mod json;
#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod listener;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "linux")]
pub mod namespaces;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod native;
pub mod runtime;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod streams;
mod strings;
pub use runtime::{Runtime, Sandbox};
