//! Responsibility-scoped implementation modules and reexports.
#[cfg(unix)]
pub(crate) mod batch;
pub(crate) mod error;
pub(crate) mod init;
#[cfg(unix)]
pub(crate) mod metadata;
#[cfg(unix)]
pub(crate) mod namespace;
#[cfg(unix)]
pub(crate) mod scan;
#[cfg(unix)]
pub(crate) mod source;
pub(crate) mod work;
