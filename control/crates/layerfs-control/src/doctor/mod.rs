//! The six doctors (call surface 2.0): `home`, `machine`, `store`,
//! `project`, `sandbox` and `backup doctor` answer one report ([`report`])
//! made by one runner ([`frame`]) over each kind's checks ([`kinds`]). A
//! doctor changes nothing and starts no machine; it takes no lock, so a
//! home that changes meanwhile may be reported as it was part way.
//!
//! It also lists what interrupted operations left on `native` and on each
//! running machine (`home leftovers`, [`leftover_list`], one bounded walk
//! per side), and `home prune` removes one of them under its guard
//! ([`leftover_prune`]): the one call here that changes anything
//! (layerfs-project#180). No doctor's report holds them.
mod capacity;
mod checks;
mod convert;
pub mod leftover_list;
pub mod leftover_prune;
pub mod leftovers;
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
