//! Store Init, sealing and one-attempt installation over the shared
//! libraries, and the projects of a live Store (#506).
mod init;
mod install;
mod install_types;
mod live;
pub use init::{InitError, InitFailure, InitRequest, ProjectApi, SealedProject};
pub use install::{install, install_file};
pub use install_types::{InstallError, InstallFailure, InstallWork, Installed};
pub use live::{projects, AddFailure, AddRequest, AddedProject, CreateRequest};
