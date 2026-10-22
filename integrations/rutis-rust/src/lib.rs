//! LayerFS operations as a rutis plugin.
//!
//! Tier 0: an already installed project, exposed as `fs.*`, `branch.*` and
//! `project.list_branches` through one `call` method.
//!
//! Tier 1: `workspace_mount`, `workspace_exec`, `workspace_status`,
//! `workspace_commit` and `workspace_unmount`, over the same napi-free session
//! layer the JavaScript binding uses. No Init, no daemon in the data path and
//! no Node runtime.
//!
//! The plugin runs in the host process and opens the Store directly; it never
//! proxies another process's Store calls.

mod config;
mod error;
mod plugin;
mod store;
mod value;

pub use config::{Config, Deploy};
pub use error::LayerFsError;
pub use plugin::LayerFsPlugin;
pub use store::{Info, LayerFs};
