//! Metadata-only immutable phase boundary, supplied by the owning adapter.
use crate::error::ContentResult;

use super::{PageLimit, StateCapacity, StateKey, StatePage, StateRecord, StateScope, StateSeal};

/// One selected append-only state authority; no SQL or native I/O enters C1.
pub trait IndexedState {
    /// The exact scope's previously admitted logical class, before canonical effects.
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity>;
    /// One acknowledged ordered batch, at most 128 records and 64 KiB with headers.
    fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> ContentResult<()>;
    /// Freeze once; deny subsequent mutation before effects.
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal>;
    /// One immutable exact key, under the acknowledged seal.
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>>;
    /// One bounded advancing page. Sealed count establishes exact EOF.
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage>;
    /// Complete this logical phase once and close further access. Native session
    /// close/unlink and physical refund belong to its caller's separate owner.
    fn release(&mut self, scope: &StateScope) -> ContentResult<()>;
}
