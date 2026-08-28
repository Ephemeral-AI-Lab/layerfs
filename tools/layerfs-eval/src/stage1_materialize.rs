//! Stage One materialization fixtures, campaigns, rows, and evidence.

mod acceptance;
mod attribution;
mod contract;
mod error;
mod evidence;
mod manifest;
mod parity;
mod prepare;
mod row;
mod trusted;

pub use acceptance::acceptance_run;
pub use attribution::{attribution_block, attribution_run, trusted_block};
pub use manifest::{hash, manifest};
pub use parity::{parity_readiness, parity_row, parity_run};
pub use prepare::prepare;
pub use trusted::trusted_run;
