//! Bounded immutable-pack saves through the two persistence ports.
mod operation;
mod pooled;
mod provider;
mod register;
mod seal;
mod select;
mod source;
mod state;
mod wave;
pub use operation::{Save, SaveSink};
pub use state::WriteOutcome;
