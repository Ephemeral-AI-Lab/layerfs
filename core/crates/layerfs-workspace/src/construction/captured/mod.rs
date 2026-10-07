//! Exact captured final-state file normalization and one consuming construction.
mod context;
mod normalize;
mod owner;
mod scan;
mod source;
mod state;
pub use owner::{CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits, CapturedFileWork};
