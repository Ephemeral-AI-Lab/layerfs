//! `apple-container` (Apple's `container` CLI; Apple silicon, macOS only).
mod adapter;
pub(crate) mod lifecycle;
pub(crate) mod status;
mod profile;
pub(crate) use adapter::ADAPTER;
pub(crate) use lifecycle::{create, remove};
pub(crate) use status::{guest, status};
pub(crate) use lifecycle::{start, stop};
pub(crate) use profile::PROFILE;
