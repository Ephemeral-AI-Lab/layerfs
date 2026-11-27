//! `host`: Sandboxes on the host's own OS (not the host itself).
mod adapter;
pub(crate) mod status;
pub(crate) mod lifecycle;
mod profile;
pub(crate) use adapter::ADAPTER;
pub(crate) use status::status;
pub(crate) use lifecycle::start;
pub(crate) use profile::PROFILE;
