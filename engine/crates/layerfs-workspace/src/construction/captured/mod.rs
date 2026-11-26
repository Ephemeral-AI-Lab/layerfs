//! Exact captured final-state file normalization and one consuming construction.
mod context;
mod edges;
pub(crate) mod kept;
mod normalize;
mod owner;
mod scan;
mod script;
mod source;
mod state;
pub use owner::{CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits, CapturedFileWork};
