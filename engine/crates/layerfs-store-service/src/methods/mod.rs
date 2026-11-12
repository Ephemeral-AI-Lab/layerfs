//! The SDK Store-call facade: one file per public Store method (`file/`,
//! `branch/`, `project`), each reading its arguments (`args`) and running the
//! engine's typed calls; [`call`] is their method table (`dispatch`).
mod args;
mod branch;
mod dispatch;
mod file;
mod project;
mod storage_report;
mod store;
pub use dispatch::call;
