//! Bounded immutable-pack saves through the two persistence ports.
mod operation;
mod pooled;
mod provider;
mod register;
mod seal;
mod select;
mod source;
mod state;
mod upload;
mod wave;
pub use operation::{Save, SaveSink};
pub use state::WriteOutcome;

mod work;
pub use work::{SaveHistory, SaveWork, StageWork};
pub(crate) use work::{Stage, Work};
