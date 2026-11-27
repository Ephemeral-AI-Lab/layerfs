//! Machine lifecycle (owner 2026-11-27): the calls that change whether a
//! machine exists or runs, one file per call, and what they share (`boot`,
//! `progress`).
#[cfg(feature = "machine")]
pub(crate) mod boot;
#[cfg(feature = "machine")]
pub(crate) mod create;
#[cfg(feature = "machine")]
pub(crate) mod force_stop;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod host;
#[cfg(feature = "machine")]
pub(crate) mod progress;
#[cfg(feature = "machine")]
pub(crate) mod remove;
#[cfg(feature = "machine")]
pub(crate) mod start;
#[cfg(feature = "machine")]
pub(crate) mod stop;
