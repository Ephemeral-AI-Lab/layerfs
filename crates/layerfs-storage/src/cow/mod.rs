//! Structural copy-on-write objects and canonical directory trees.

#![allow(unused_imports)]

pub(crate) mod file;
mod mutate;
mod tree;
mod view;

#[cfg(feature = "operation-polymorphism")]
pub(crate) use mutate::{
    add_directory_entry_cow_borrowed_v1, move_directory_entry_cow_borrowed_v1,
    remove_directory_entry_cow_borrowed_v1, replace_directory_entry_cow_borrowed_v1,
    replace_two_directory_entries_cow_borrowed_v1,
};
#[cfg(test)]
pub(crate) use mutate::{
    add_directory_entry_cow_v1, remove_directory_entry_cow_v1, replace_directory_entry_cow_v1,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use tree::build_canonical_directory_borrowed_v1;
#[cfg(test)]
pub(crate) use tree::build_canonical_directory_v1;
pub(crate) use tree::{
    preflight_canonical_tree_v1, CanonicalDirectoryTreeV1, CanonicalTreeChildV1,
    CanonicalTreeEntryV1, CanonicalTreeShapeV1, CowTreeMutationV1, CowTreeReplacementV1,
    DirectoryBuildModeV1, DirectoryLogicalIdentityV1, PreparedTreeSinkV1, TreeObjectDispositionV1,
    TreePageBoundaryV1, TreePageSummaryV1, TreeSinkErrorV1, MAX_COW_TREE_PAGE_SUMMARIES,
    MAX_DIRECTORY_HASH_PROOF_NODES, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use view::{
    mutation_evidence_resident_bytes_v1, mutation_hash_state_bytes_v1,
    replacement_evidence_resident_bytes_v1,
};
pub(crate) use view::{
    AuthenticatedTreeMutationEvidenceV1, AuthenticatedTreeReplacementEvidenceV1,
    CanonicalTreeMutationSourceV1, DirectoryHashProofV1, DirectoryHashSubtreeV1,
    DirectoryMutationHashProofV1, TreeMutationSourceErrorV1,
};
