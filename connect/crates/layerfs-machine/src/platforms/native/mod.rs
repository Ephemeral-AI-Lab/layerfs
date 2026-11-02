//! `native`: the host itself.
mod observe;
mod power;
mod profile;
pub(crate) use observe::status;
pub(crate) use power::start;
pub(crate) use profile::PROFILE;
