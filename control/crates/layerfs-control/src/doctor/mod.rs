//! The six doctors (call surface 2.0): `home`, `machine`, `store`,
//! `project`, `sandbox` and `backup doctor` answer one report ([`report`])
//! made by one runner ([`frame`]) over each kind's checks ([`kinds`]). A
//! doctor changes nothing and starts no machine. It waits for no lock: the
//! one lock it takes is the host worker probe's single non-blocking try of
//! the startup lock and `worker.lock`, released at once and only for a
//! socket that has no listener or lacks its private mode (`machine get
//! --with live` shares that probe). A home that changes meanwhile may be
//! reported as it was part way.
//!
//! No doctor's report holds what interrupted operations left: LayerFS
//! removes it itself where nothing can own it (spec v20,
//! `layerfs_sandbox::sweep` and the sweeps of image copies and backups).
mod capacity;
mod checks;
mod mount_holders;
mod records;
mod scan;

mod check_report;
pub mod frame;
mod kinds;
pub mod report;
pub mod target;

pub use frame::{run, run_with, CheckId, KindDoctor, Located, Options, Probe, Registry};
pub use kinds::kinds;
pub use report::{Kind, Node, Report, Thing};

mod host_target;
#[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
pub(crate) mod host_worker;
mod machine_target;
mod network;
pub(crate) use network::{inspect_in_worker as inspect_network_in_worker, OP as NETWORK_OP};
mod project_target;
mod sandbox_target;
mod workspace_target;
pub(crate) use sandbox_target::worker_diagnose as diagnose_worker_sandbox;
pub(crate) use scan::shell;
mod backup_target;
mod store_target;
pub(crate) use store_target::in_worker as diagnose_store_in_worker;
