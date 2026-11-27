//! Public Project organization over host Init, a project's home entry and
//! discovery records (`record`), and the project calls that check who uses
//! a project (`with_users`).
#[doc(hidden)]
pub mod delete;
#[doc(hidden)]
pub mod export;
#[doc(hidden)]
pub mod frame;
#[doc(hidden)]
pub mod hide_path;
#[doc(hidden)]
pub mod init;
#[doc(hidden)]
pub mod record;
#[doc(hidden)]
pub mod status;
pub mod with_users;
pub use layerfs_project::store::{
    InitError, InitFailure, InitRequest, InstallError, InstallFailure, InstallWork, Installed,
    ProjectApi, SealedProject,
};
