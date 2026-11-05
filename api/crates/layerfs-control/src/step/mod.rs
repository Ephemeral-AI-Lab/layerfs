//! The step runtime (#555): a launcher-owned helper outside the Sandbox
//! compute plane and the daemon.
//!
//! It starts one command asynchronously, keeps the wait in a durable entry
//! under the home's `step/` folder, reads the status, cancels by id and
//! never retries or replays an unknown outcome. It introduces no core queue
//! or actor and changes no daemon protocol: the daemon keeps its records,
//! admission and capacity role.
//!
//! [`StepRuntime`] is the use-case surface; [`Launcher`] is the one port a
//! concrete launcher adapter implements (the real one is
//! [`ProcessLauncher`]), and [`WaitStore`] is the one durable backend.
//! [`ExecRequest`], [`ExecId`], [`ExecStatus`], [`ExecReceipt`] and
//! [`CancelOutcome`] are the contract types.
mod contract;
mod family;
mod launcher;
mod process;
mod runtime;
mod store;

pub use contract::{
    CancelOutcome, ExecId, ExecReceipt, ExecRequest, ExecStatus, Started, MAXIMUM_KEY_BYTES,
};
pub use family::step;
pub use launcher::{Clock, Launcher, Observation, SystemClock};
pub use process::ProcessLauncher;
pub use runtime::StepRuntime;
pub use store::WaitStore;
