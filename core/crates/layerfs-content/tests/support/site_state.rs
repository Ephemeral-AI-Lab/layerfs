//! Finite external semantic oracle; its resident maps are not a physical proof.
#![allow(dead_code)]
use layerfs_content::filesystem::rows::{BindingPoint, BindingSourceId};
use layerfs_content::filesystem::state::*;
use layerfs_content::{ContentError, ContentResult};
use std::cell::Cell;
use std::collections::BTreeMap;

pub fn scopes(source: BindingSourceId) -> SiteConstructionScopes {
    let mut selection = StateSelection::issue([0x61; 32]).unwrap();
    selection.bind_owner([0x82; 32]).unwrap();
    SiteConstructionScopes::new(selection, source).unwrap()
}
pub fn reference_scope(scope: &SiteScope) -> [u8; 89] {
    let mut bytes = [0; 89];
    bytes[..32].copy_from_slice(scope.state().selection().selector());
    bytes[32..40].copy_from_slice(&scope.state().selection().token().to_be_bytes());
    bytes[40..72].copy_from_slice(scope.state().selection().owner_binding().unwrap());
    bytes[72..80].copy_from_slice(&scope.state().phase().to_be_bytes());
    bytes[80] = 3;
    bytes[81..89].copy_from_slice(&scope.source_id().as_bytes());
    bytes
}
pub fn reference_point(
    source: BindingSourceId,
    parent: u64,
    descriptor: u64,
    ordinal: u32,
) -> [u8; 28] {
    let mut bytes = [0; 28];
    bytes[..8].copy_from_slice(&source.as_bytes());
    bytes[8..16].copy_from_slice(&parent.to_be_bytes());
    bytes[16..24].copy_from_slice(&descriptor.to_be_bytes());
    bytes[24..28].copy_from_slice(&ordinal.to_be_bytes());
    bytes
}
pub fn reference_record(
    scope: &SiteScope,
    serial: u64,
    point: BindingPoint,
    flags: u8,
) -> [u8; 60] {
    let mut bytes = [0; 60];
    bytes[..2].copy_from_slice(&25u16.to_be_bytes());
    bytes[2..10].copy_from_slice(&scope.state().selection().token().to_be_bytes());
    bytes[10..18].copy_from_slice(&scope.state().phase().to_be_bytes());
    bytes[18] = 3;
    bytes[19..27].copy_from_slice(&serial.to_be_bytes());
    bytes[27..31].copy_from_slice(&29u32.to_be_bytes());
    bytes[31] = flags;
    bytes[32..60].copy_from_slice(&reference_point(
        scope.source_id(),
        point.parent(),
        point.header_descriptor(),
        point.binding_ordinal(),
    ));
    bytes
}
pub fn reference_seal(scope: &SiteScope, rows: &[SiteRecord], birth: bool) -> [u8; 138] {
    let context = reference_scope(scope);
    let mut hash = blake3::Hasher::new();
    hash.update(if birth {
        b"layerfs/binding-sites/birth/v1\0"
    } else {
        b"layerfs/binding-sites/final/v1\0"
    });
    hash.update(&context);
    for row in rows {
        hash.update(&reference_record(
            scope,
            row.key().serial(),
            row.point(),
            if birth {
                u8::from(row.has_base())
            } else {
                row.flags()
            },
        ));
    }
    let count = rows.len() as u64;
    let bytes = count * 60;
    hash.update(&count.to_be_bytes());
    hash.update(&bytes.to_be_bytes());
    let mut seal = [0; 138];
    seal[0] = 1;
    seal[1..90].copy_from_slice(&context);
    seal[90..98].copy_from_slice(&count.to_be_bytes());
    seal[98..106].copy_from_slice(&bytes.to_be_bytes());
    seal[106..138].copy_from_slice(hash.finalize().as_bytes());
    seal
}

pub struct ObservedSites {
    pub scope: SiteScope,
    pub rows: BTreeMap<SiteKey, SiteRecord>,
    pub roots: ResidentState,
    pub stage: u8,
    pub abandoned: bool,
    pub batches: Vec<usize>,
    pub parent_queries: u64,
    pub parent_pages: u64,
    pub observed: Vec<SiteObservation>,
    pub final_pages: u64,
    pub retirements: u64,
    pub abandonments: u64,
    pub root_capacities: Cell<u64>,
    pub peak_page: usize,
    pub declared: usize,
    pub membership: Option<SiteMembership>,
    pub final_seal: Option<SiteSeal>,
    pub fail_at: Option<&'static str>,
    pub corrupt_maximum: bool,
    pub corrupt_page: bool,
    pub corrupt_birth_at_final: bool,
}
impl ObservedSites {
    pub fn new(scopes: &SiteConstructionScopes, directories: usize, bindings: usize) -> Self {
        Self {
            scope: scopes.sites().clone(),
            rows: BTreeMap::new(),
            roots: ResidentState::new(scopes.roots().clone(), directories).unwrap(),
            stage: 0,
            abandoned: false,
            batches: Vec::new(),
            parent_queries: 0,
            parent_pages: 0,
            observed: Vec::new(),
            final_pages: 0,
            retirements: 0,
            abandonments: 0,
            root_capacities: Cell::new(0),
            peak_page: 0,
            declared: bindings,
            membership: None,
            final_seal: None,
            fail_at: None,
            corrupt_maximum: false,
            corrupt_page: false,
            corrupt_birth_at_final: false,
        }
    }
    fn check_scope(&self, scope: &SiteScope) -> ContentResult<()> {
        if scope != &self.scope {
            return Err(ContentError::InvalidOrderingRecord("oracle site scope"));
        }
        if self.abandoned {
            return Err(ContentError::InvalidOrderingRecord(
                "oracle abandoned sites",
            ));
        }
        Ok(())
    }
    fn operation(&self, name: &'static str) -> ContentResult<()> {
        if self.fail_at == Some(name) {
            return Err(ContentError::ProviderFailure { what: name });
        }
        Ok(())
    }
    fn facts(&self, members: &SiteMembership) -> ContentResult<()> {
        self.check_scope(members.birth().scope())?;
        if self.stage != 1 || self.membership.as_ref() != Some(members) {
            return Err(ContentError::InvalidOrderingRecord("oracle membership"));
        }
        Ok(())
    }
    fn roots_ready(&self) -> ContentResult<()> {
        if self.stage != 4 || self.abandoned {
            return Err(ContentError::InvalidOrderingRecord("sites not retired"));
        }
        Ok(())
    }
    fn birth(&self) -> SiteBirthSeal {
        let mut rows: Vec<_> = self.rows.values().copied().collect();
        rows.sort_by_key(|row| (row.point().parent(), row.point().binding_ordinal()));
        SiteBirthSeal::decode(&self.scope, &reference_seal(&self.scope, &rows, true)).unwrap()
    }
}
impl BindingSiteState for ObservedSites {
    fn site_capacity(&self, scope: &SiteScope) -> ContentResult<SiteCapacity> {
        self.check_scope(scope)?;
        if self.stage != 0 {
            return Err(ContentError::InvalidOrderingRecord("oracle birth closed"));
        }
        SiteCapacity::new(self.declared as u64, self.declared as u64 * 60)
    }
    fn site_get(&mut self, scope: &SiteScope, key: SiteKey) -> ContentResult<Option<SiteRecord>> {
        self.check_scope(scope)?;
        if self.stage > 1 {
            return Err(ContentError::InvalidOrderingRecord("oracle get phase"));
        }
        SiteKey::decode(scope, key.as_bytes())?;
        self.operation("site presence")?;
        Ok(self.rows.get(&key).copied())
    }
    fn site_insert_batch(
        &mut self,
        scope: &SiteScope,
        rows: &[SiteRecord],
    ) -> ContentResult<ClaimAdmission> {
        self.check_scope(scope)?;
        if self.stage != 0 {
            return Err(ContentError::InvalidOrderingRecord("oracle birth closed"));
        }
        self.operation("site append")?;
        assert!(rows.len() <= 128);
        let mut keys = std::collections::BTreeSet::new();
        let mut points = std::collections::BTreeSet::new();
        for row in rows {
            SiteRecord::decode(
                scope,
                &reference_record(scope, row.key().serial(), row.point(), row.flags()),
            )?;
            if !row.is_birth() {
                return Err(ContentError::InvalidOrderingRecord("oracle birth flags"));
            }
            let point = (row.point().parent(), row.point().binding_ordinal());
            if !keys.insert(row.key())
                || !points.insert(point)
                || self.rows.contains_key(&row.key())
                || self
                    .rows
                    .values()
                    .any(|old| (old.point().parent(), old.point().binding_ordinal()) == point)
            {
                self.abandoned = true;
                return Ok(ClaimAdmission::Duplicate);
            }
        }
        if self.rows.len() + rows.len() > self.declared {
            return Err(ContentError::InvalidOrderingRecord("oracle declared sites"));
        }
        self.batches.push(rows.len());
        self.rows.extend(rows.iter().map(|row| (row.key(), *row)));
        Ok(ClaimAdmission::Fresh)
    }
    fn site_close_membership(&mut self, expected: &SiteBirthSeal) -> ContentResult<SiteMembership> {
        self.check_scope(expected.scope())?;
        if self.stage != 0 || self.birth() != *expected {
            return Err(ContentError::InvalidOrderingRecord("oracle birth digest"));
        }
        self.operation("site close")?;
        let maximum = self.rows.keys().next_back().copied();
        let maximum = if self.corrupt_maximum && self.rows.len() > 1 {
            self.rows.keys().next().copied()
        } else {
            maximum
        };
        let members = SiteMembership::new(expected.clone(), maximum)?;
        self.stage = 1;
        self.membership = Some(members.clone());
        Ok(members)
    }
    fn site_parent_present(
        &mut self,
        members: &SiteMembership,
        parent: u64,
    ) -> ContentResult<bool> {
        self.facts(members)?;
        self.operation("site parent")?;
        self.parent_queries += 1;
        Ok(self
            .rows
            .values()
            .any(|row| row.has_base() && row.point().parent() == parent))
    }
    fn site_parent_page(
        &mut self,
        members: &SiteMembership,
        parent: u64,
        after: Option<u32>,
        limit: SiteParentPageLimit,
    ) -> ContentResult<SiteParentPage> {
        self.facts(members)?;
        self.operation("site parent page")?;
        self.parent_pages += 1;
        let mut all: Vec<_> = self
            .rows
            .values()
            .filter(|row| row.has_base() && row.point().parent() == parent)
            .map(|row| row.birth_projection())
            .collect();
        all.sort_by_key(|row| row.point().binding_ordinal());
        let maximum = all.last().map(|row| row.point().binding_ordinal());
        if after != maximum && limit.fitting_records() == 0 {
            limit.check_records(1)?;
        }
        let mut rows = Vec::with_capacity(limit.fitting_records());
        rows.extend(
            all.into_iter()
                .filter(|row| after.is_none_or(|last| row.point().binding_ordinal() > last))
                .take(limit.fitting_records()),
        );
        self.peak_page = self.peak_page.max(rows.len());
        let last = rows
            .last()
            .map(|row| row.point().binding_ordinal())
            .or(after);
        SiteParentPage::after(
            members.clone(),
            parent,
            after,
            maximum,
            rows,
            last == maximum,
        )
    }
    fn site_observe_base_batch(
        &mut self,
        members: &SiteMembership,
        observations: &[SiteObservation],
    ) -> ContentResult<()> {
        self.facts(members)?;
        self.operation("site facts")?;
        assert!(observations.len() <= 128);
        let mut next = self.rows.clone();
        for observation in observations {
            let row =
                next.get_mut(&observation.key())
                    .ok_or(ContentError::InvalidOrderingRecord(
                        "oracle missing observation",
                    ))?;
            *row = row.observe(observation.legal())?;
        }
        self.rows = next;
        self.observed.extend_from_slice(observations);
        Ok(())
    }
    fn site_final_seal(&mut self, members: &SiteMembership) -> ContentResult<SiteSeal> {
        self.facts(members)?;
        self.operation("site final")?;
        if self.corrupt_birth_at_final || self.birth() != *members.birth() {
            return Err(ContentError::InvalidOrderingRecord(
                "oracle immutable birth",
            ));
        }
        let rows: Vec<_> = self.rows.values().copied().collect();
        let seal = SiteSeal::decode(&self.scope, &reference_seal(&self.scope, &rows, false))?;
        self.stage = 2;
        self.final_seal = Some(seal.clone());
        Ok(seal)
    }
    fn site_sealed_page(
        &mut self,
        seal: &SiteSeal,
        after: Option<SiteKey>,
        limit: SitePageLimit,
    ) -> ContentResult<SitePage> {
        self.check_scope(seal.scope())?;
        if self.stage != 2 || self.final_seal.as_ref() != Some(seal) {
            return Err(ContentError::InvalidOrderingRecord(
                "oracle final selection",
            ));
        }
        self.operation("site page")?;
        self.final_pages += 1;
        let maximum = self.rows.keys().next_back().copied();
        if maximum != after && limit.fitting_records() == 0 {
            limit.check_records(1)?;
        }
        let mut rows = Vec::with_capacity(limit.fitting_records());
        rows.extend(
            self.rows
                .values()
                .filter(|row| after.is_none_or(|last| row.key() > last))
                .take(limit.fitting_records())
                .copied(),
        );
        if self.corrupt_page && !rows.is_empty() {
            rows[0] = rows[0].observe(true)?;
        }
        self.peak_page = self.peak_page.max(rows.len());
        let last = rows.last().map(|row| row.key()).or(after);
        SitePage::after(seal.clone(), after, rows, last == maximum)
    }
    fn site_retire(&mut self, seal: &SiteSeal) -> ContentResult<()> {
        self.check_scope(seal.scope())?;
        if self.stage != 2 || self.final_seal.as_ref() != Some(seal) {
            return Err(ContentError::InvalidOrderingRecord(
                "oracle retire selection",
            ));
        }
        self.operation("site retire")?;
        self.retirements += 1;
        self.rows.clear();
        self.stage = 4;
        Ok(())
    }
    fn site_abandon(&mut self, scope: &SiteScope) -> ContentResult<()> {
        if scope != &self.scope {
            return Err(ContentError::InvalidOrderingRecord("oracle abandon scope"));
        }
        self.abandonments += 1;
        self.abandoned = true;
        Ok(())
    }
}
impl IndexedState for ObservedSites {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.root_capacities.set(self.root_capacities.get() + 1);
        self.roots_ready()?;
        self.roots.capacity(scope)
    }
    fn append(&mut self, scope: &StateScope, rows: &[StateRecord]) -> ContentResult<()> {
        self.roots_ready()?;
        self.roots.append(scope, rows)
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.roots_ready()?;
        self.roots.seal(scope)
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.roots_ready()?;
        self.roots.get(seal, key)
    }
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.roots_ready()?;
        self.roots.page(seal, after, limit)
    }
    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.roots_ready()?;
        self.roots.release(scope)
    }
}
