//! Reads at a saved snapshot: typed file, listing, search, compare and blame
//! reads over a Branch, `main`, or a Commit or Layer identity, with bounded
//! windows and the call caches they share.
pub mod attribution;
pub mod blame;
pub mod branch_compare;
pub mod cached;
pub mod compare;
pub mod comparison_pages;
pub mod file_compare;
pub mod file_facts;
pub(crate) mod line_alignment;
pub mod paged_reads;
mod paged_boundaries;
pub mod plans;
pub mod query;
pub(crate) mod read_reply;
pub mod reads;
pub mod revision;
pub(crate) mod revision_pages;
pub mod search;
pub mod snapshot;
pub mod walk;
pub mod window;
