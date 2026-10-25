//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod backing;
pub(crate) mod batch;
pub(crate) mod error;
pub(crate) mod files;
pub(crate) mod hints;
pub(crate) mod host;
pub(crate) mod init;
pub(crate) mod namespace;
pub(crate) mod observe;
pub(crate) mod scan;
pub(crate) mod source;
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod windows_identity;
pub(crate) mod work;
