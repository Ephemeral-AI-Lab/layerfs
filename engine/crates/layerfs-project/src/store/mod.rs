//! Store Init, sealing and one-attempt installation over the shared
//! libraries, and the projects of a live Store (#506).
mod init;
mod install;
pub(crate) mod install_outcome;
mod live;
mod live_open;
pub use init::{InitError, InitFailure, InitRequest, ProjectApi, SealedProject};
pub use install::{install, install_file};
pub use install_outcome::{InstallError, InstallFailure, InstallWork, Installed};
pub use live::{projects, AddFailure, AddRequest, AddedProject, CreateRequest};
