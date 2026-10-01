//! Typed directory roots and exclusive binding claims with exact phase custody.
//!
//! Declarations and reexports only; native/physical ownership stays external.
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
