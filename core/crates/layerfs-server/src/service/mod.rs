//! Authorized operation code: admission, C1/C2/C5 operations and Init.
//!
//! Declarations and reexports only; the handler, its Project Init adapter and
//! the read/save operation modules own the behavior.
pub(crate) mod admission;
pub(crate) mod construction;
mod construction_drain;
pub(crate) mod construction_file;
pub(crate) mod construction_session;
pub(crate) mod empty_admission;
pub(crate) mod empty_receive;
pub(crate) mod error;
pub(crate) mod handler;
pub(crate) mod init_project;
pub(crate) mod input;
pub(crate) mod read;
pub(crate) mod records;
pub(crate) mod save;
pub(crate) mod small_file_admission;
pub(crate) mod small_file_receive;
pub use handler::{Grant, Service, StoreAccess};
