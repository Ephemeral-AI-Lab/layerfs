//! `native`: the host itself.
mod observe;
mod power;
pub(crate) use observe::status;
pub(crate) use power::start;
