//! Explicit resident claim compatibility, separate from the bounded provider path.
use super::{
    BindingClaimState, ClaimAdmission, ClaimCapacity, ClaimKey, ClaimLedger, ClaimPage,
    ClaimPageLimit, ClaimRecord, ClaimSeal, StateScope, StateTable, CLAIM_RECORD_BYTES,
    STATE_MAX_PAGE_RECORDS,
};
use crate::error::{ContentError, ContentResult};
use std::collections::BTreeSet;

/// Caller-admitted resident compatibility. This is not a paged population proof.
pub struct ResidentClaims {
    scope: StateScope,
    capacity: ClaimCapacity,
    keys: BTreeSet<ClaimKey>,
    seal: Option<ClaimSeal>,
    retired: bool,
    abandoned: bool,
}
impl ResidentClaims {
    /// Selects the explicit legacy logical population before claim mutation.
    pub fn new(scope: StateScope, maximum_records: usize) -> ContentResult<Self> {
        if scope.table() != StateTable::BindingClaims {
            return Err(ContentError::InvalidOrderingRecord("claim table"));
        }
        let count = u64::try_from(maximum_records).map_err(|_| ContentError::LengthOverflow)?;
        let bytes = count
            .checked_mul(CLAIM_RECORD_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(Self {
            scope,
            capacity: ClaimCapacity::new(count, bytes)?,
            keys: BTreeSet::new(),
            seal: None,
            retired: false,
            abandoned: false,
        })
    }
    fn selected(&self, scope: &StateScope) -> ContentResult<()> {
        if self.retired || self.abandoned || scope != &self.scope {
            return Err(ContentError::InvalidOrderingRecord("claim authority"));
        }
        Ok(())
    }
    fn open(&self, scope: &StateScope) -> ContentResult<()> {
        self.selected(scope)?;
        if self.seal.is_some() {
            return Err(ContentError::InvalidOrderingRecord("claim sealed mutation"));
        }
        Ok(())
    }
    fn sealed(&self, seal: &ClaimSeal) -> ContentResult<()> {
        self.selected(seal.scope())?;
        if self.seal.as_ref() != Some(seal) {
            return Err(ContentError::InvalidOrderingRecord(
                "claim sealed selection",
            ));
        }
        Ok(())
    }
}
impl BindingClaimState for ResidentClaims {
    fn claim_capacity(&self, scope: &StateScope) -> ContentResult<ClaimCapacity> {
        self.open(scope)?;
        Ok(self.capacity)
    }
    fn claim_present(&mut self, scope: &StateScope, key: ClaimKey) -> ContentResult<bool> {
        self.open(scope)?;
        ClaimKey::decode(scope, key.as_bytes())?;
        Ok(self.keys.contains(&key))
    }
    fn claim_batch(
        &mut self,
        scope: &StateScope,
        keys: &[ClaimKey],
    ) -> ContentResult<ClaimAdmission> {
        self.open(scope)?;
        if keys.len() > STATE_MAX_PAGE_RECORDS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.append_records",
                limit: STATE_MAX_PAGE_RECORDS as u64,
                actual: keys.len() as u64,
            });
        }
        for key in keys {
            ClaimKey::decode(scope, key.as_bytes())?;
        }
        for (index, key) in keys.iter().enumerate() {
            if keys[..index].contains(key) || self.keys.contains(key) {
                self.abandoned = true;
                return Ok(ClaimAdmission::Duplicate);
            }
        }
        let actual = self
            .keys
            .len()
            .checked_add(keys.len())
            .ok_or(ContentError::LengthOverflow)?;
        self.capacity.check_requested(actual)?;
        self.keys.extend(keys.iter().copied());
        Ok(ClaimAdmission::Fresh)
    }
    fn claim_seal(&mut self, scope: &StateScope) -> ContentResult<ClaimSeal> {
        self.open(scope)?;
        let mut ledger = ClaimLedger::new(scope.clone())?;
        for key in &self.keys {
            ledger.acknowledge(&[ClaimRecord::new(*key)])?;
        }
        let seal = ledger.seal();
        self.seal = Some(seal.clone());
        Ok(seal)
    }
    fn claim_page(
        &mut self,
        seal: &ClaimSeal,
        after: Option<ClaimKey>,
        limit: ClaimPageLimit,
    ) -> ContentResult<ClaimPage> {
        self.sealed(seal)?;
        if let Some(key) = after {
            ClaimKey::decode(&self.scope, key.as_bytes())?;
        }
        let fitting = limit.fitting_records();
        if fitting == 0 && after != self.keys.last().copied() {
            return Err(ContentError::InvalidOrderingRecord("claim page progress"));
        }
        let mut records = Vec::new();
        records
            .try_reserve_exact(fitting)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "binding_claims.page_window",
            })?;
        use std::ops::Bound::{Excluded, Unbounded};
        let range = after.map_or(Unbounded, Excluded);
        for key in self.keys.range((range, Unbounded)).take(fitting) {
            records.push(ClaimRecord::new(*key));
        }
        let last = records.last().map(|record| record.key()).or(after);
        let eof = last == self.keys.last().copied();
        let page = ClaimPage::after(seal.clone(), after, records, eof)?;
        page.check_limit(limit)?;
        Ok(page)
    }
    fn claim_abandon(&mut self, scope: &StateScope) -> ContentResult<()> {
        if scope != &self.scope {
            return Err(ContentError::InvalidOrderingRecord("claim authority"));
        }
        self.abandoned = true;
        self.keys = BTreeSet::new();
        Ok(())
    }

    fn claim_retire(&mut self, seal: &ClaimSeal) -> ContentResult<()> {
        self.sealed(seal)?;
        self.retired = true;
        self.keys = BTreeSet::new();
        Ok(())
    }
}
