//! Host-direct agent SDK. Project initialization is the only live method.
mod client;
pub use client::Client;
pub use layerfs_api_core::{Error, Project, Workspace};
