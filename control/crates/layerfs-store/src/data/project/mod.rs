//! Public Project organization over host Init, and a project's home entry
//! and discovery records (`discovery`).
pub mod delete;
pub mod export;
pub mod frame;
pub mod hide_path;
pub mod init;
pub mod record;
pub mod status;
pub mod with_users;
pub use layerfs_project::store::{
    InitError, InitFailure, InitRequest, InstallError, InstallFailure, InstallWork, Installed,
    ProjectApi, SealedProject,
};

// The project calls that check who uses a project (`with_users`).
