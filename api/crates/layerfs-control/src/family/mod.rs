//! The 2.0 family functions (#533): each resolves its records, chooses a
//! route and runs today's Store or host operation once.
pub mod backups;
pub(crate) mod deploy;
pub mod files;
pub mod postgres;
pub mod projects;
pub mod sandboxes;
pub mod stores;
