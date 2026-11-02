//! `firecracker`: one Firecracker microVM per machine inside a `lima` host.
mod create;
mod host;
mod observe;
mod power;
mod profile;
mod scripts;
pub(crate) use create::{create, remove};
pub(crate) use host::{firecracker_files, FIRECRACKER_DIR, FIRECRACKER_GUEST};
pub(crate) use observe::{machine_guest, status};
pub(crate) use power::{start, stop};
pub(crate) use profile::PROFILE;
