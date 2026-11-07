//! `firecracker`: one Firecracker microVM per machine inside a `lima` host.
#[cfg(unix)]
mod adapter;
#[cfg(unix)]
mod create;
#[cfg(unix)]
mod host;
#[cfg(unix)]
mod observe;
#[cfg(unix)]
mod power;
mod profile;
#[cfg(unix)]
mod scripts;
#[cfg(unix)]
pub(crate) use adapter::ADAPTER;
#[cfg(unix)]
pub(crate) use create::{create, remove};
#[cfg(unix)]
pub(crate) use host::{firecracker_files, FIRECRACKER_DIR, FIRECRACKER_GUEST};
#[cfg(unix)]
pub(crate) use observe::{machine_guest, status};
#[cfg(unix)]
pub(crate) use power::{start, stop};
pub(crate) use profile::PROFILE;
