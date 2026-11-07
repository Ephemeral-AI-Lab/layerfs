//! Public Project organization over host Init, and a project's home entry
//! and discovery records (`discovery`).
pub mod add;
pub mod discovery;
#[doc(hidden)]
pub mod family;
pub mod frame;
pub mod init;
pub mod parts;
pub mod postgres_init;
#[doc(hidden)]
pub mod record;
pub mod remove;
pub mod send;
pub mod staged;
pub mod tree;
pub use layerfs_project::store::{
    InitError, InitFailure, InitRequest, InstallError, InstallFailure, InstallWork, Installed,
    ProjectApi, SealedProject,
};
