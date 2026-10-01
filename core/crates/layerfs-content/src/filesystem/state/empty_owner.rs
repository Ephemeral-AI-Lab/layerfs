//! A real nonnative ALL-ZERO owner with fixed admitted phase authority.
use super::*;
use crate::filesystem::rows::VerifiedEmptyRows;
use crate::{ContentError, ContentResult, ObjectId};

pub(super) struct EmptyData {
    pub(super) selection: StateSelection,
    pub(super) scopes: GraphConstructionScopes,
    pub(super) confirmed: bool,
    pub(super) failed: bool,
    pub(super) sites_closed: bool,
    pub(super) members: Option<SiteMembership>,
    pub(super) site_seal: Option<SiteSeal>,
    pub(super) site_eof: bool,
    pub(super) sites_retired: bool,
    pub(super) alias_begun: bool,
    pub(super) alias_seal: Option<AliasSeal>,
    pub(super) alias_retired: bool,
    pub(super) graph_stage: GraphStage,
    pub(super) adjacency: Option<GraphAdjacencySeal>,
    pub(super) adjacency_eof: bool,
    pub(super) solving_eof: bool,
    pub(super) proof: Option<GraphProofSeal>,
    pub(super) proof_eof: bool,
    pub(super) roots_seal: Option<StateSeal>,
    pub(super) roots_eof: bool,
    pub(super) roots_released: bool,
    pub(super) facts: Option<FactScope>,
    pub(super) fact_seal: Option<FactSeal>,
    pub(super) fact_eof: bool,
    pub(super) facts_retired: bool,
    pub(super) parents: Option<FactScope>,
    pub(super) parents_closed: bool,
    pub(super) parent_seal: Option<ParentSeal>,
    pub(super) parent_eof: bool,
    pub(super) parents_retired: bool,
}
/// Fixed truthful operation resources, distinct from the captured logicalS.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmptyPhysical {
    /// Native owner binding is absent; StateSelection uses an explicit logical binding.
    pub native_binding: Option<[u8; 32]>,
    /// Metadata native file count.
    pub native_files: u64,
    /// Actual metadata native reservation.
    pub reserved_bytes: u64,
    /// Actual metadata native allocated bytes.
    pub allocated_bytes: u64,
    /// Actual native close/unlink/reset events.
    pub cleanup_events: u64,
}
/// Verified source-limited authority for the same checked namespace coordinator.
pub struct VerifiedEmptyState {
    pub(super) data: Box<EmptyData>,
    pub(super) memory: GraphMemory,
    // Box data dies before its last-owner credit is returned.
    _owner: GraphMemoryLease,
}
impl VerifiedEmptyState {
    /// Capture UpdateBase/source/root and bind a real logical owner once.
    pub fn new(selector: [u8; 32], subject: GraphSubject) -> ContentResult<Self> {
        Self::new_with_memory(selector, subject, GraphMemory::new())
    }
    pub(super) fn new_with_memory(
        selector: [u8; 32],
        subject: GraphSubject,
        memory: GraphMemory,
    ) -> ContentResult<Self> {
        if subject.mode() != GraphMode::Update {
            return Err(bad("empty state requires UpdateBase"));
        }
        let owner = memory.reserve(std::mem::size_of::<EmptyData>() + 128)?;
        Self::new_with_credit(selector, subject, owner)
    }
    pub(super) fn new_with_credit(
        selector: [u8; 32],
        subject: GraphSubject,
        owner: GraphMemoryLease,
    ) -> ContentResult<Self> {
        if subject.mode() != GraphMode::Update
            || owner.bytes() < std::mem::size_of::<EmptyData>() + 128
        {
            return Err(bad("empty transferred owner admission"));
        }
        let memory = owner.memory().clone();
        let mut selection = StateSelection::issue(selector)?;
        // Fixed transient stack framing;128 prepaid bytes cover the issued Arc
        // control/fields in addition to the separately admitted boxed data.
        let mut binding = [0u8; 256];
        let mut used = 0;
        let domain = b"layerfs/verified-empty/logical-owner/v1\0";
        let token = selection.token().to_be_bytes();
        let encoded_subject = subject.encode();
        for part in [
            domain.as_slice(),
            selector.as_slice(),
            token.as_slice(),
            encoded_subject.as_slice(),
        ] {
            binding[used..used + part.len()].copy_from_slice(part);
            used += part.len();
        }
        selection.bind_owner(*ObjectId::for_bytes(&binding[..used]).as_bytes())?;
        let scopes = GraphConstructionScopes::new(selection.clone(), subject)?;
        Ok(Self {
            data: Box::new(EmptyData {
                selection,
                scopes,
                confirmed: false,
                failed: false,
                sites_closed: false,
                members: None,
                site_seal: None,
                site_eof: false,
                sites_retired: false,
                alias_begun: false,
                alias_seal: None,
                alias_retired: false,
                graph_stage: GraphStage::Deferred,
                adjacency: None,
                adjacency_eof: false,
                solving_eof: false,
                proof: None,
                proof_eof: false,
                roots_seal: None,
                roots_eof: false,
                roots_released: false,
                facts: None,
                fact_seal: None,
                fact_eof: false,
                facts_retired: false,
                parents: None,
                parents_closed: false,
                parent_seal: None,
                parent_eof: false,
                parents_retired: false,
            }),
            memory,
            _owner: owner,
        })
    }
    /// Source confirmation can only use the concrete original all-zero EOF proof.
    pub fn confirm(&mut self, rows: &VerifiedEmptyRows) -> ContentResult<()> {
        if rows.source_id() != self.subject().source_id() {
            return Err(bad("empty state foreign source"));
        }
        if self.data.confirmed || self.data.failed {
            return Err(bad("empty state confirmation replay"));
        }
        self.data.confirmed = true;
        Ok(())
    }
    pub(super) fn confirm_small_file(
        &mut self,
        rows: &crate::filesystem::rows::VerifiedSmallFileRows,
    ) -> ContentResult<()> {
        if rows.source_id() != self.subject().source_id()
            || rows.subject() != self.subject()
            || rows.is_empty()
            || rows.len() > 8
            || self.data.confirmed
            || self.data.failed
        {
            return Err(bad("small file selected proof"));
        }
        // This concrete proof's constructor authenticated actual table and every
        // existing file kind before exact declared EOF. No count/bool adoption.
        self.data.confirmed = true;
        Ok(())
    }
    /// The exact issued logical selection, never a native path/header adoption.
    pub fn selection(&self) -> &StateSelection {
        &self.data.selection
    }
    /// Exact captured immutable source/base/namespace/root/logical class.
    pub fn subject(&self) -> &GraphSubject {
        self.data.scopes.graph().subject()
    }
    /// The same current Sites/Graph/Roots scoped coordinator interfaces.
    pub fn scopes(&self) -> &GraphConstructionScopes {
        &self.data.scopes
    }
    /// Current first-party working admission including still-live result credits.
    pub fn working_bytes(&self) -> usize {
        self.memory.reserved_bytes()
    }
    /// Truthful actual native resources of this nonnative authority.
    pub const fn physical(&self) -> EmptyPhysical {
        EmptyPhysical {
            native_binding: None,
            native_files: 0,
            reserved_bytes: 0,
            allocated_bytes: 0,
            cleanup_events: 0,
        }
    }
    /// All required logical phase completions, before canonical filesystem-root output.
    pub fn completed(&self) -> bool {
        !self.data.failed
            && self.data.roots_released
            && self.data.facts_retired
            && self.data.parents_retired
    }
    /// Pure own-operation failure custody, no native effects or guessed refunds.
    pub fn abandon(&mut self) {
        self.data.failed = true;
    }
    pub(super) fn live(&self) -> ContentResult<()> {
        if !self.data.confirmed || self.data.failed {
            return Err(bad("empty state unconfirmed/failed"));
        }
        Ok(())
    }
    pub(super) fn site_scope(&self, scope: &SiteScope) -> ContentResult<()> {
        if scope != self.data.scopes.sites() {
            return Err(bad("empty foreign Sites"));
        }
        self.live()
    }
    pub(super) fn graph_scope(&self, scope: &GraphScope) -> ContentResult<()> {
        if scope != self.data.scopes.graph() {
            return Err(bad("empty foreign Graph"));
        }
        self.live()
    }
    pub(super) fn roots_scope(&self, scope: &StateScope) -> ContentResult<()> {
        if scope != self.data.scopes.roots() {
            return Err(bad("empty foreign Roots"));
        }
        self.live()?;
        if self.data.graph_stage != GraphStage::Retired
            || self.data.roots_released
            || !self.data.facts_retired
            || !self.data.parents_closed
            || self.data.parent_seal.is_none()
        {
            return Err(bad("empty Roots phase"));
        }
        Ok(())
    }
    pub(super) fn members(&self, members: &SiteMembership) -> ContentResult<()> {
        self.site_scope(members.birth().scope())?;
        if self.data.members.as_ref() != Some(members) || self.data.sites_retired {
            return Err(bad("empty selected membership"));
        }
        Ok(())
    }
}
pub(super) fn bad(what: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(what)
}
pub(super) fn growth(actual: usize) -> ContentResult<()> {
    if actual != 0 {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "verified_empty.growth",
            limit: 0,
            actual: actual as u64,
        });
    }
    Ok(())
}
