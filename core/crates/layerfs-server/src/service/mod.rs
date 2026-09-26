//! Authorized operation code: admission, C1/C2/C5 operations and Init.
//!
//! Declarations and reexports only; the handler, its Project Init adapter and
//! the read/save operation modules own the behavior.
pub(crate) mod error;
pub(crate) mod handler;
pub(crate) mod init_project;
pub(crate) mod input;
pub(crate) mod read;
pub(crate) mod records;
pub(crate) mod save;
pub use handler::{Grant, Service, StoreAccess};
