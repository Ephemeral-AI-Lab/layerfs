//! Host-direct agent SDK. Project initialization is the only live method.
mod client;
mod host;
pub use client::Client;
pub use host::Host;
pub use layerfs_api_core::{Error, Project, Workspace};
