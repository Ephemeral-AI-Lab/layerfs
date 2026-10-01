//! One bounded source-order birth producer and independent final verifier.
use super::{
    BindingSiteState, ClaimAdmission, SiteBirthLedger, SiteCursor, SiteKey, SiteMembership,
    SitePageLimit, SiteRecord, SiteScope, STATE_MAX_PAGE_RECORDS,
};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::rows::{BindingPoint, DirectoryHeader, PreparedBindingRows};
use std::collections::BTreeMap;

pub(crate) struct BindingSites<'a, S: BindingSiteState + ?Sized> {
    state: &'a mut S,
    ledger: SiteBirthLedger,
    pending: Vec<SiteRecord>,
    declared: u64,
    active_stored: u64,
}
impl<'a, S: BindingSiteState + ?Sized> BindingSites<'a, S> {
    pub(crate) fn new(state: &'a mut S, scope: SiteScope, declared: usize) -> ContentResult<Self> {
        state.site_capacity(&scope)?.check_requested(declared)?;
        let mut pending = Vec::new();
        let capacity = declared.min(STATE_MAX_PAGE_RECORDS);
        pending
            .try_reserve_exact(capacity)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "binding_sites.pending_window",
            })?;
        if pending.capacity() > capacity {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.pending_capacity",
                limit: capacity as u64,
                actual: pending.capacity() as u64,
            });
        }
        Ok(Self {
            state,
            ledger: SiteBirthLedger::new(scope)?,
            pending,
            declared: u64::try_from(declared).map_err(|_| ContentError::LengthOverflow)?,
            active_stored: 0,
        })
    }
    pub(crate) fn claim(
        &mut self,
        serial: u64,
        header: &DirectoryHeader,
        ordinal: u32,
        has_base: bool,
        active: bool,
    ) -> ContentResult<()> {
        let key = SiteKey::new(self.ledger.scope(), serial)?;
        if self.pending.iter().any(|record| record.key() == key) {
            return Err(ContentError::InvalidRecord("multiple parents"));
        }
        if let Some(record) = self.state.site_get(self.ledger.scope(), key)? {
            SiteRecord::decode(self.ledger.scope(), &record.encode())?;
            if record.key() != key || !record.is_birth() {
                return Err(ContentError::InvalidOrderingRecord("site selected key"));
            }
            return Err(ContentError::InvalidRecord("multiple parents"));
        }
        let actual = self
            .ledger
            .records()
            .checked_add(self.pending.len() as u64)
            .and_then(|count| count.checked_add(1))
            .ok_or(ContentError::LengthOverflow)?;
        if actual > self.declared {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.declared_records",
                limit: self.declared,
                actual,
            });
        }
        let record = SiteRecord::birth(
            self.ledger.scope(),
            serial,
            BindingPoint::new(header, ordinal)?,
            has_base,
        )?;
        self.pending.push(record);
        if has_base && active {
            self.active_stored = self
                .active_stored
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?;
        }
        if self.pending.len() == STATE_MAX_PAGE_RECORDS {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> ContentResult<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.ledger.validate_append(&self.pending)?;
        match self
            .state
            .site_insert_batch(self.ledger.scope(), &self.pending)?
        {
            ClaimAdmission::Fresh => self.ledger.acknowledge(&self.pending)?,
            ClaimAdmission::Duplicate => {
                return Err(ContentError::InvalidRecord("multiple parents"))
            }
        }
        self.pending.clear();
        Ok(())
    }
    pub(crate) fn close_membership(mut self) -> ContentResult<ClosedSites<'a, S>> {
        self.flush()?;
        self.pending = Vec::new();
        let expected = self.ledger.seal();
        let members = self.state.site_close_membership(&expected)?;
        if members.birth() != &expected || members.maximum() != self.ledger.maximum() {
            return Err(ContentError::InvalidOrderingRecord(
                "site acknowledged membership",
            ));
        }
        // No birth hasher or producer-window owner survives the Facts handoff.
        Ok(ClosedSites {
            state: self.state,
            members,
            active_stored: self.active_stored,
        })
    }
}

pub(crate) struct ClosedSites<'a, S: BindingSiteState + ?Sized> {
    state: &'a mut S,
    members: SiteMembership,
    active_stored: u64,
}
impl<S: BindingSiteState + ?Sized> ClosedSites<'_, S> {
    pub(crate) fn members(&self) -> &SiteMembership {
        &self.members
    }
    pub(crate) fn state(&mut self) -> &mut S {
        self.state
    }
    pub(crate) fn active_stored(&self) -> u64 {
        self.active_stored
    }
    pub(crate) fn finish(
        self,
        input: &dyn PreparedBindingRows,
        unreachable: &BTreeMap<u64, ()>,
        members: &SiteMembership,
    ) -> ContentResult<()> {
        if members != &self.members {
            return Err(ContentError::InvalidOrderingRecord(
                "site selected membership",
            ));
        }
        let seal = self.state.site_final_seal(members)?;
        if seal.scope() != members.birth().scope()
            || seal.records() != members.birth().records()
            || seal.encoded_bytes() != members.birth().encoded_bytes()
        {
            return Err(ContentError::InvalidOrderingRecord(
                "site acknowledged final seal",
            ));
        }
        let mut cursor = SiteCursor::new(self.state, seal.clone())?;
        while let Some(page) = cursor.next_page(SitePageLimit::default())? {
            for record in page.records() {
                if !record.has_base() || unreachable.contains_key(&record.point().parent()) {
                    continue;
                }
                let (_, child) = input.binding_at(&record.point())?;
                if child != Some(record.key().serial()) {
                    return Err(ContentError::InvalidRecord("binding replay"));
                }
                if record.saw_base() && !record.any_legal_base() {
                    return Err(ContentError::InvalidRecord("multiple parents"));
                }
            }
        }
        drop(cursor);
        self.state.site_retire(&seal)
    }
}
