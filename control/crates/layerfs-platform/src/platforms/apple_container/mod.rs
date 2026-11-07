//! `apple-container` (Apple's `container` CLI; Apple silicon, macOS only).
mod adapter;
mod create;
mod observe;
mod power;
mod profile;
pub(crate) use adapter::ADAPTER;
pub(crate) use create::{create, remove};
pub(crate) use observe::{guest, status};
pub(crate) use power::{start, stop};
pub(crate) use profile::PROFILE;
