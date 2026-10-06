//! Responsibility-scoped implementation modules and reexports.
#[cfg(unix)]
pub(crate) mod backing;
#[cfg(unix)]
pub(crate) mod batch;
pub(crate) mod error;
#[cfg(unix)]
pub(crate) mod files;
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
