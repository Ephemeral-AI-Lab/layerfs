//! Commit methods: one Commit, a Branch's Commits and their differences.
mod diff;
mod get;
mod list;
pub use diff::diff;
pub use get::get;
pub use list::list;
