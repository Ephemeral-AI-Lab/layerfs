//! Authenticated bounded reads over the persistence ports.

mod length;
mod objects;
pub(crate) mod prefetch;
mod provider;
pub(crate) mod source;

pub use objects::call_failure;
pub(crate) use prefetch::chains as prefetch_chains;
pub use provider::{KeptReads, ReadCache, Reader};
pub use source::captured::CapturedLocators;
pub(crate) use source::captured::LocatorMode;
pub use source::counters::Diagnostics;
pub(crate) use source::fetch::Fetch;
pub(crate) use source::hints::hint_failure;
