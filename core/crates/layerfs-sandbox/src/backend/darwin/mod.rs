//! Native macOS (`aarch64-apple-darwin`) process Sandbox backend.
mod exec;
mod sandbox;

pub use super::*;
pub type Darwin = Docker;
pub(crate) use sandbox::{connect, is_darwin_root};
