//! The six doctors (call surface 2.0): `home`, `machine`, `store`,
//! `project`, `sandbox` and `backup doctor` answer one report ([`report`])
//! made by one runner ([`frame`]) over each kind's checks ([`kinds`]). A
//! doctor changes nothing and starts no machine; it takes no lock, so a
//! home that changes meanwhile may be reported as it was part way.
//!
//! It also lists what interrupted operations left on `native` and on each
//! running machine ([`leftovers()`], one bounded walk per side), and
//! `doctor.fix` removes one of them under its guard (`leftover_fix`): the
//! one call here that changes anything (layerfs-project#180). No doctor's
//! report holds them.
// the whole-home sweep's live parts (a Sandbox daemon's capacity, each
// worker's build) left with it; the Sandbox and machine packages fold them
// into their own checks
#[allow(dead_code)]
mod capacity;
mod checks;
mod convert;
pub mod leftover_fix;
pub mod leftover_report;
pub mod leftovers;
mod records;
mod scan;
#[allow(dead_code)]
mod workers;

pub use records::leftovers;

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
mod sandbox_target;
pub(crate) use sandbox_target::worker_diagnose as diagnose_worker_sandbox;
mod backup_target;
mod store_target;
pub(crate) use store_target::in_worker as diagnose_store_in_worker;
