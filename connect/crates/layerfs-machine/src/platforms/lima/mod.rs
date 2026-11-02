//! `lima` (`limactl`, `vmType: vz`, macOS only).
mod create;
mod observe;
mod power;
mod template;
pub(crate) use create::{create, remove};
pub(crate) use observe::{guest, status};
pub(crate) use power::{start, stop};
