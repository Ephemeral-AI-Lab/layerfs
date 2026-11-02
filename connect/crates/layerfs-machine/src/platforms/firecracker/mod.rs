//! `firecracker`: one Firecracker microVM per machine inside a `lima` host.
mod create;
mod observe;
mod power;
mod scripts;
pub(crate) use create::{create, remove};
pub(crate) use observe::{machine_guest, status};
pub(crate) use power::{start, stop};
