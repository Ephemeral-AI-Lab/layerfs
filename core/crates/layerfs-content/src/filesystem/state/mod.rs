//! Typed directory roots and exclusive binding claims with exact phase custody.
//!
//! Declarations and reexports only; native/physical ownership stays external.
mod alias_compat;
mod alias_construction;
mod alias_port;
mod alias_progress;
mod alias_records;
mod alias_resident;
mod binding_claims;
mod binding_sites;
mod claim_cursor;
mod claim_page;
mod claim_port;
mod claim_records;
mod claim_resident;
mod claim_seal;
mod construction;
mod cursor;
mod directory_roots;
mod graph;
mod page;
mod port;
mod records;
mod resident;
mod seal;
mod selection;
mod site_birth;
mod site_construction;
mod site_cursor;
mod site_membership;
mod site_page;
mod site_parent_cursor;
mod site_parent_page;
mod site_port;
mod site_records;
mod site_scope;
mod site_seal;

pub(crate) use binding_claims::BindingClaims;
pub(crate) use binding_sites::BindingSites;
pub use claim_cursor::ClaimCursor;
pub use claim_page::{ClaimPage, ClaimPageLimit, CLAIM_PAGE_HEADER_BYTES};
pub use claim_port::{BindingClaimState, ClaimAdmission};
pub use claim_records::{
    ClaimCapacity, ClaimKey, ClaimRecord, CLAIM_APPEND_HEADER_BYTES, CLAIM_RECORD_BYTES,
};
pub use claim_resident::ResidentClaims;
pub use claim_seal::{ClaimLedger, ClaimSeal, CLAIM_SEAL_BYTES};
pub use construction::{ConstructionScopes, ConstructionState};
pub use cursor::StateCursor;
pub(crate) use directory_roots::DirectoryRoots;
pub use graph::*;
pub use page::{PageLimit, StatePage, STATE_PAGE_HEADER_BYTES};
pub use port::IndexedState;
pub use records::{
    StateCapacity, StateKey, StateRecord, DIRECTORY_ROOT_RECORD_BYTES, STATE_APPEND_HEADER_BYTES,
    STATE_KEY_BYTES, STATE_MAX_KEY_BYTES, STATE_MAX_PAGE_BYTES, STATE_MAX_PAGE_RECORDS,
    STATE_MAX_VALUE_BYTES,
};
pub use resident::ResidentState;
pub use seal::{StateLedger, StateSeal, STATE_SEAL_BYTES};
pub use selection::{StateScope, StateSelection, StateTable, STATE_SCOPE_BYTES};

pub use site_birth::{SiteBirthLedger, SiteBirthSeal, SITE_BIRTH_SEAL_BYTES};
pub use site_construction::{SiteConstructionScopes, SiteConstructionState};
pub use site_cursor::SiteCursor;
pub use site_membership::{SiteMembership, SITE_MEMBERSHIP_BYTES};
pub use site_page::{SitePage, SitePageLimit, SITE_PAGE_HEADER_BYTES};
pub use site_parent_cursor::SiteParentCursor;
pub use site_parent_page::{SiteParentPage, SiteParentPageLimit, SITE_PARENT_PAGE_HEADER_BYTES};
pub use site_port::BindingSiteState;
pub use site_records::{
    SiteCapacity, SiteKey, SiteObservation, SiteRecord, SITE_APPEND_HEADER_BYTES, SITE_RECORD_BYTES,
};
pub use site_scope::{SiteScope, SITE_SCOPE_BYTES};
pub use site_seal::{SiteLedger, SiteSeal, SITE_SEAL_BYTES};

pub(crate) use alias_compat::CompatibilityAliases;
pub use alias_construction::AliasGraphConstructionState;
pub use alias_port::{AliasCurrent, AliasFrontier, AliasSeal};
pub use alias_progress::{AliasProgress, ALIAS_PROGRESS_BYTES};
pub use alias_records::{
    AliasCapacity, AliasFact, ALIAS_FACT_BYTES, ALIAS_FIXED_BYTES, ALIAS_JOB_BYTES,
};
pub use alias_resident::ResidentAliasFrontier;

mod empty_facts;
mod empty_graph;
mod empty_owner;
mod empty_roots;
mod empty_sites;
mod fact_ledger;
mod fact_port;
mod fact_records;
mod fact_scope;
mod namespace_construction;
mod parent_access;
pub use empty_owner::{EmptyPhysical, VerifiedEmptyState};
pub use fact_ledger::FactLedger;
pub use fact_port::{
    FactPage, FactSeal, FactState, ParentEligibilityState, ParentSeal, FACT_PAGE_HEADER_BYTES,
};
pub use fact_records::{
    BaseFact, FactCapacity, FactOccupancy, ParentFact, BASE_FACT_BYTES, FACT_OWNER_ALLOWANCE,
    PARENT_ELIGIBILITY_BYTES,
};
pub use fact_scope::{FactScope, FactSubject, FACT_SCOPE_BYTES, FACT_SUBJECT_BYTES};
pub use namespace_construction::NamespaceConstructionState;
pub(crate) use parent_access::{EligibilityAuthority, EligibilityView, ParentCalls};

mod canonical_capacity;
mod canonical_construction;
mod canonical_scope;
mod count_page;
mod count_port;
mod count_records;
mod count_seal;
mod release_name;
mod release_port;
mod release_records;
mod root_cursor;
pub use canonical_capacity::{
    CanonicalCapacity, CanonicalOccupancy, CANONICAL_OWNER_ALLOWANCE, COUNT_RECORD_BYTES,
    RELEASE_FRAME_BYTES, RELEASE_JOB_BYTES, ZERO_SEED_BYTES,
};
pub use canonical_construction::CanonicalConstructionState;
pub use canonical_scope::{CanonicalScope, CANONICAL_SCOPE_BYTES};
pub use count_page::{CountPage, ZeroPage, COUNT_PAGE_HEADER_BYTES, ZERO_PAGE_HEADER_BYTES};
pub use count_port::CountState;
pub use count_records::{CountEpoch, CountRecord, COUNT_VALUE_BYTES};
pub use count_seal::{
    CountLedger, CountSeal, ZeroLedger, ZeroSeal, COUNT_SEAL_BYTES, ZERO_SEAL_BYTES,
};
pub use directory_roots::{directory_root_cursor_working_bytes, directory_roots_working_bytes};
pub use release_name::ReleaseName;
pub use release_port::ReleaseState;
pub use release_records::{ReleaseFrame, ReleaseJob, ReleaseSeal};
pub(crate) use root_cursor::RootCursor;

mod small_file_counts;
mod small_file_facts;
mod small_file_namespace;
mod small_file_owner;
mod small_file_release;
mod small_file_roots;
pub use small_file_owner::VerifiedSmallFileState;
