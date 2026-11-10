//! `layerfs doctor` (#536 H7): a report of what is wrong in a home, which
//! changes nothing and starts no machine. It reads the records, their modes
//! and references, the `op` intents, the Store leases, a layout 2 home's
//! conversion blockers, partial Store copies on `native` and the build of
//! each running machine's worker (its hello alone: no request is sent).
//! Each finding names the call that fixes it. It takes no lock: a home
//! that changes meanwhile may be reported as it was part way.
mod checks;
mod convert;
mod report;
mod scan;
mod workers;

pub use report::{doctor, doctor_json};

mod check_report;
pub mod target;

mod dispatch;
pub use dispatch::doctor_target;
pub use target::{DoctorRequest, DoctorTarget};

mod host_target;
mod machine_target;
mod sandbox_target;
pub(crate) use sandbox_target::worker_diagnose as diagnose_worker_sandbox;
mod backup_target;
mod store_target;
pub(crate) use store_target::in_worker as diagnose_store_in_worker;
