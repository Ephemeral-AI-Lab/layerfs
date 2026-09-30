//! Closed exclusive-binding claim operations, supplied by the owning adapter.
use super::{ClaimCapacity, ClaimKey, ClaimPage, ClaimPageLimit, ClaimSeal, StateScope};
use crate::error::ContentResult;

/// Known semantic outcome; provider and rollback failures remain errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimAdmission {
    /// Every key was absent and the whole batch was acknowledged once.
    Fresh,
    /// A key was already claimed; this batch was rolled back without additions.
    Duplicate,
}

/// The selected claim phase; C1 has no SQL or native I/O authority.
pub trait BindingClaimState {
    /// Previously admitted exact logical class before claim effects.
    fn claim_capacity(&self, scope: &StateScope) -> ContentResult<ClaimCapacity>;
    /// Checks exact key/class presence in the open phase, preserving error order.
    fn claim_present(&mut self, scope: &StateScope, key: ClaimKey) -> ContentResult<bool>;
    /// Unordered <=128 keys; Fresh acknowledges all, Duplicate acknowledges none.
    fn claim_batch(
        &mut self,
        scope: &StateScope,
        keys: &[ClaimKey],
    ) -> ContentResult<ClaimAdmission>;
    /// Freezes the exact final ordered population once.
    fn claim_seal(&mut self, scope: &StateScope) -> ContentResult<ClaimSeal>;
    /// One bounded indexed page; acknowledged maximum establishes exact EOF.
    fn claim_page(
        &mut self,
        seal: &ClaimSeal,
        after: Option<ClaimKey>,
        limit: ClaimPageLimit,
    ) -> ContentResult<ClaimPage>;
    /// Terminalizes this exact failed attempt without SQL, native cleanup or retry.
    /// An already failed selected attempt is logically idempotent; Unknown stays retained.
    fn claim_abandon(&mut self, scope: &StateScope) -> ContentResult<()>;
    /// Retires only the acknowledged exact seal; known completion enables roots.
    fn claim_retire(&mut self, seal: &ClaimSeal) -> ContentResult<()>;
}
