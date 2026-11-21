//! The six doctors (call surface 2.0): `home`, `machine`, `store`,
//! `project`, `sandbox` and `backup doctor` answer one report ([`report`])
//! made by one runner ([`frame`]) over each kind's checks ([`kinds`]). A
//! doctor changes nothing and starts no machine; it takes no lock, so a
//! home that changes meanwhile may be reported as it was part way.
//!
//! No doctor's report holds what interrupted operations left: LayerFS
//! removes it itself where nothing can own it (spec v20,
//! `layerfs_sandbox::sweep` and the sweeps of image copies and backups).
mod capacity;
mod checks;
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
mod machine_target;
mod project_target;
mod sandbox_target;
mod workspace_target;
pub(crate) use sandbox_target::worker_diagnose as diagnose_worker_sandbox;
pub(crate) use scan::shell;
mod backup_target;
mod store_target;
pub(crate) use store_target::in_worker as diagnose_store_in_worker;

mod mount_holders;
