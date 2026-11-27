//! The SDK Store-call facade: one file per public Store method (`file/`,
//! `branch/`, `project`), each reading its arguments (`args`) and running the
//! engine's typed calls; [`call`] is their method table (`dispatch`).
mod args;
mod branch;
pub(crate) mod commit;
mod dispatch;
mod file;
pub mod operator;
mod store;
pub use crate::filesystem::change::run::Source;
pub use dispatch::{call, call_streamed};
