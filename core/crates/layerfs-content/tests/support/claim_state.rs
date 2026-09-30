//! External dispatch observation over the actual explicit resident provider.
#![allow(dead_code)]
use std::cell::Cell;
use std::rc::Rc;

use layerfs_content::filesystem::state::*;
use layerfs_content::{ContentError, ContentResult};

pub fn scopes() -> ConstructionScopes {
    let mut selection = StateSelection::issue([0x51; 32]).unwrap();
    selection.bind_owner([0x72; 32]).unwrap();
    ConstructionScopes::new(selection).unwrap()
}

pub struct ObservedState {
    pub claims: ResidentClaims,
    pub roots: ResidentState,
    pub retired: Rc<Cell<bool>>,
    pub batches: Vec<usize>,
    pub presence: u64,
    pub seals: u64,
    pub pages: u64,
    pub retirements: u64,
    pub abandonments: u64,
    abandoned: bool,
    pub root_capacities: Cell<u64>,
    pub maximum_page: usize,
    pub refuse_batch: bool,
    pub refuse_retire: bool,
}
impl ObservedState {
    pub fn new(scopes: &ConstructionScopes, directories: usize, bindings: usize) -> Self {
        Self {
            claims: ResidentClaims::new(scopes.claims().clone(), bindings).unwrap(),
            roots: ResidentState::new(scopes.roots().clone(), directories).unwrap(),
            retired: Rc::new(Cell::new(false)),
            batches: Vec::new(),
            presence: 0,
            seals: 0,
            pages: 0,
            retirements: 0,
            abandonments: 0,
            abandoned: false,
            root_capacities: Cell::new(0),
            maximum_page: 0,
            refuse_batch: false,
            refuse_retire: false,
        }
    }
    fn roots_ready(&self) -> ContentResult<()> {
        if !self.retired.get() || self.abandoned {
            return Err(ContentError::InvalidOrderingRecord("claims not retired"));
        }
        Ok(())
    }
}
impl BindingClaimState for ObservedState {
    fn claim_capacity(&self, scope: &StateScope) -> ContentResult<ClaimCapacity> {
        self.claims.claim_capacity(scope)
    }
    fn claim_present(&mut self, scope: &StateScope, key: ClaimKey) -> ContentResult<bool> {
        self.presence += 1;
        self.claims.claim_present(scope, key)
    }
    fn claim_batch(
        &mut self,
        scope: &StateScope,
        keys: &[ClaimKey],
    ) -> ContentResult<ClaimAdmission> {
        self.batches.push(keys.len());
        if self.refuse_batch {
            return Err(ContentError::ProviderFailure {
                what: "claim acknowledgement",
            });
        }
        self.claims.claim_batch(scope, keys)
    }
    fn claim_seal(&mut self, scope: &StateScope) -> ContentResult<ClaimSeal> {
        self.seals += 1;
        self.claims.claim_seal(scope)
    }
    fn claim_page(
        &mut self,
        seal: &ClaimSeal,
        after: Option<ClaimKey>,
        limit: ClaimPageLimit,
    ) -> ContentResult<ClaimPage> {
        self.pages += 1;
        let page = self.claims.claim_page(seal, after, limit)?;
        self.maximum_page = self.maximum_page.max(page.records().len());
        Ok(page)
    }
    fn claim_abandon(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.abandonments += 1;
        self.claims.claim_abandon(scope)?;
        self.abandoned = true;
        Ok(())
    }
    fn claim_retire(&mut self, seal: &ClaimSeal) -> ContentResult<()> {
        self.retirements += 1;
        if self.refuse_retire {
            return Err(ContentError::ProviderFailure {
                what: "claim retirement",
            });
        }
        self.claims.claim_retire(seal)?;
        self.retired.set(true);
        Ok(())
    }
}
impl IndexedState for ObservedState {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.root_capacities.set(self.root_capacities.get() + 1);
        self.roots_ready()?;
        self.roots.capacity(scope)
    }
    fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> ContentResult<()> {
        self.roots_ready()?;
        self.roots.append(scope, records)
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
