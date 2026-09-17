//! Timing scopes, completed reports and their renderers.
//!
//! This entry module declares the timer modules and re-exports their public
//! surface. Types, state, bounds, control flow and formatting live in the
//! dedicated sibling modules.
//!
//! [`Timing`](crate::timer::Timing) starts or disables a recording and hands out
//! a [`TimingScope`](crate::timer::TimingScope) to the operation. A pending scope
//! is single-use: calling
//! [`run`](crate::timer::TimingScope::run) starts its timer, runs the operation
//! and returns the unchanged `Result`. Child scopes are created from the handle
//! that `run` passes to the operation, so a child can never outlive its parent's
//! measured region. Measured data is rendered by
//! [`write_text`](crate::timer::TimingReport::write_text) or
//! [`write_json`](crate::timer::TimingReport::write_json).

mod format;
mod json;
mod recording;
mod report;
mod scope;

pub use recording::{MAX_DEPTH, MAX_LABEL_BYTES, MAX_NODES};
pub use report::{Completeness, NodeOutcome, TimingNode, TimingReport};
pub use scope::{Active, Pending, Timing, TimingScope};
