//! Store Init, sealing and one-attempt installation over the shared libraries.
mod init;
mod install;
mod install_types;
pub use init::{InitError, InitFailure, InitRequest, ProjectApi, SealedProject};
pub use install::{install, install_file};
pub use install_types::{InstallError, InstallFailure, InstallWork, Installed};
