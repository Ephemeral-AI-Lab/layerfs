//! Authenticated bounded reads over the persistence ports.

mod counters;
mod fetch;
mod length;
mod locator_cache;
mod objects;
mod prefetch;
mod provider;
mod representation;
mod units;

pub use counters::Diagnostics;
pub(crate) use fetch::Fetch;
pub use provider::{KeptReads, ReadCache, Reader};
pub use representation::{is_payload, DescribedObject, PayloadRecord};
