//! Public Project organization over native Init and authenticated daemon control.
mod api;
mod history;
pub(crate) mod init;
mod init_types;
pub(crate) mod install;
mod install_types;
pub use api::ProjectApi;
pub use init::initialize;
pub use init_types::{InitError, InitFailure, InitRequest, SealedProject};
pub use install::install;
pub use install_types::{InstallError, InstallFailure, InstallWork, Installed};
