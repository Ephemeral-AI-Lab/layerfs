//! Responsibility-scoped implementation modules and reexports.
#[cfg(unix)]
pub(crate) mod backing;
#[cfg(unix)]
pub(crate) mod batch;
pub(crate) mod error;
#[cfg(unix)]
pub(crate) mod files;
pub(crate) mod hints;
#[cfg(unix)]
pub(crate) mod namespace;
pub(crate) mod observe;
pub(crate) mod run;
#[cfg(unix)]
pub(crate) mod scan;
#[cfg(unix)]
pub(crate) mod source;
