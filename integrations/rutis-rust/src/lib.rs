//! LayerFS Store operations as a rutis plugin.
//!
//! Tier 0: an already installed project, exposed as `fs.*`, `branch.*` and
//! `project.list_branches` through one `call` method. No Init, no Workspace,
//! no daemon and no Node runtime.
//!
//! The plugin runs in the host process and opens the Store directly with
//! `layerfs-store-service`; it never proxies another process's Store calls.

mod config;
mod error;
mod plugin;
mod store;
mod value;

pub use config::Config;
pub use error::LayerFsError;
pub use plugin::LayerFsPlugin;
pub use store::{Info, LayerFs};
