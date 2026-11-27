//! Authenticated one-time Store file installation into this daemon.
mod file;
pub mod outcome;
mod receive;
pub use outcome::StoreSettings;
pub use receive::{receive_install, receive_install_manifest};
