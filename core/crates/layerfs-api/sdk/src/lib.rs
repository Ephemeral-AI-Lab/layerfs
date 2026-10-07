//! Host Project Init and sealed Store provisioning, with no Store data service.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod init;
mod init_types;
mod install;
mod install_types;
pub use init::initialize;
pub use init_types::{InitError, InitFailure, InitRequest, SealedProject};
pub use install::install;
pub use install_types::{InstallError, InstallFailure, InstallWork, Installed};

pub mod control;
