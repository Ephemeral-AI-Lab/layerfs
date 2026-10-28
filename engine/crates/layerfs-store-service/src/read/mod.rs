//! Reads at a saved snapshot: typed file, listing, search, compare and blame
//! reads over a Branch, `main`, or a Commit or Layer identity, with bounded
//! windows and the call caches they share.
pub mod blame;
pub mod cached;
pub mod compare;
pub mod plans;
pub mod reads;
pub mod search;
pub mod snapshot;
pub mod walk;
pub mod window;
