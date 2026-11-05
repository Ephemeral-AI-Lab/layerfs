//! `layerfs doctor` (#536 H7): a report of what is wrong in a home, which
//! changes nothing and starts no machine. It reads the records, their modes
//! and references, the `op` intents, the Store leases, a layout 2 home's
//! conversion blockers, partial Store copies on `native` and the build of
//! each running machine's worker (its hello alone: no request is sent).
//! Each finding names the call that fixes it. It takes no lock: a home
//! that changes meanwhile may be reported as it was part way.
mod checks;
mod convert;
mod host;
mod report;
mod scan;
mod workers;

pub use report::{doctor, doctor_json};
