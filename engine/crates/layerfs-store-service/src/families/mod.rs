//! The SDK Store-call facade, one folder per call family (`file/`, `branch/`,
//! `commit/`, `project`, `store`, `operator/`): one file per public Store
//! method, each reading its arguments (`args`) and running the engine's typed
//! calls; [`call`] is their method table (`dispatch`).
pub mod args;
mod branch;
mod commit;
mod dispatch;
mod file;
pub mod operator;
mod project;
mod store;
pub use crate::filesystem::change::run::Source;
pub use dispatch::{call, call_streamed};
