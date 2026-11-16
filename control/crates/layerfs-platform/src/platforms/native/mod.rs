//! `native`: the host itself.
#[cfg(unix)]
mod adapter;
#[cfg(unix)]
mod observe;
#[cfg(unix)]
mod power;
mod profile;
#[cfg(unix)]
pub(crate) use adapter::ADAPTER;
#[cfg(unix)]
#[cfg(unix)]
pub(crate) use observe::status;
#[cfg(unix)]
#[cfg(unix)]
pub(crate) use power::start;
pub(crate) use profile::PROFILE;
