//! Commit methods: one Commit, a Branch's Commits and their differences.
mod diff;
mod diff_branches;
mod diff_commits;
mod get;
mod list;
pub use diff::diff;
pub use diff_branches::diff_branches;
pub use diff_commits::diff_commits;
pub use get::get_commit;
pub use list::list_commits;
