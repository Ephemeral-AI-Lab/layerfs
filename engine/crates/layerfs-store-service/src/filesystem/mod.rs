//! Reads at a saved snapshot: typed file, listing, search, compare and blame
//! reads over a Branch, `main`, or a Commit or Layer identity, with bounded
//! windows and the call caches they share.
pub mod cache;
pub mod change;
pub mod compare;
pub mod facts;
pub mod paged;
pub mod root;
pub mod text;
pub mod tree;
pub mod versions;
