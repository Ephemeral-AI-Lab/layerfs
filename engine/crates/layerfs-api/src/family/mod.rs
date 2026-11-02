//! The 2.0 family functions (#533): each resolves its records, chooses a
//! route and runs today's Store or host operation once.
pub(crate) mod backups;
pub(crate) mod deploy;
pub(crate) mod files;
pub(crate) mod postgres;
pub(crate) mod projects;
pub(crate) mod sandboxes;
pub(crate) mod stores;
