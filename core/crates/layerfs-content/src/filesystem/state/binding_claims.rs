//! One fixed exclusive-claim producer, full verifier and known retirement boundary.
use super::{
    BindingClaimState, ClaimAdmission, ClaimCursor, ClaimKey, ClaimPageLimit, StateScope,
    CLAIM_RECORD_BYTES, STATE_MAX_PAGE_RECORDS,
};
use crate::error::{ContentError, ContentResult};

pub(crate) struct BindingClaims<'a, S: BindingClaimState + ?Sized> {
    state: &'a mut S,
    scope: StateScope,
    pending: Vec<ClaimKey>,
    declared: u64,
    acknowledged: u64,
}
impl<'a, S: BindingClaimState + ?Sized> BindingClaims<'a, S> {
    pub(crate) fn new(state: &'a mut S, scope: StateScope, declared: usize) -> ContentResult<Self> {
        state.claim_capacity(&scope)?.check_requested(declared)?;
        let mut pending = Vec::new();
        let capacity = declared.min(STATE_MAX_PAGE_RECORDS);
        pending
            .try_reserve_exact(capacity)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "binding_claims.pending_window",
            })?;
        if pending.capacity() > capacity {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.pending_capacity",
                limit: capacity as u64,
                actual: pending.capacity() as u64,
            });
        }
        Ok(Self {
            state,
            scope,
            pending,
            declared: u64::try_from(declared).map_err(|_| ContentError::LengthOverflow)?,
            acknowledged: 0,
        })
    }
    pub(crate) fn claim(&mut self, serial: u64) -> ContentResult<()> {
        let key = ClaimKey::new(&self.scope, serial)?;
        // Local and acknowledged membership are checked immediately, before
        // subsequent parent/kind checks can hide the first duplicate verdict.
        if self.pending.contains(&key) || self.state.claim_present(&self.scope, key)? {
            return Err(ContentError::InvalidRecord("multiple parents"));
        }
        let actual = self
            .acknowledged
            .checked_add(self.pending.len() as u64)
            .and_then(|n| n.checked_add(1))
            .ok_or(ContentError::LengthOverflow)?;
        if actual > self.declared {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.declared_records",
                limit: self.declared,
                actual,
            });
        }
        self.pending.push(key);
        if self.pending.len() == STATE_MAX_PAGE_RECORDS {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> ContentResult<()> {
        if !self.pending.is_empty() {
            let acknowledged = self
                .acknowledged
                .checked_add(self.pending.len() as u64)
                .ok_or(ContentError::LengthOverflow)?;
            match self.state.claim_batch(&self.scope, &self.pending)? {
                ClaimAdmission::Fresh => self.acknowledged = acknowledged,
                ClaimAdmission::Duplicate => {
                    return Err(ContentError::InvalidRecord("multiple parents"))
                }
            }
            self.pending.clear();
        }
        Ok(())
    }
    pub(crate) fn finish(mut self) -> ContentResult<()> {
        self.flush()?;
        self.pending = Vec::new();
        let seal = self.state.claim_seal(&self.scope)?;
        if seal.scope() != &self.scope
            || seal.records() != self.acknowledged
            || seal.encoded_bytes()
                != self
                    .acknowledged
                    .checked_mul(CLAIM_RECORD_BYTES as u64)
                    .ok_or(ContentError::LengthOverflow)?
        {
            return Err(ContentError::InvalidOrderingRecord(
                "claim acknowledged seal",
            ));
        }
        let mut cursor = ClaimCursor::new(self.state, seal.clone())?;
        while let Some(page) = cursor.next_page(ClaimPageLimit::default())? {
            drop(page);
        }
        drop(cursor);
        self.state.claim_retire(&seal)
    }
}
